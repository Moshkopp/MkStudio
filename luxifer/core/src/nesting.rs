//! Nesting (Material-Ausnutzung): packt die Auswahl platzsparend aufs Bett.
//!
//! Reine Geometrie (UI-frei, testbar). Stufe 1 wie in v3 erprobt:
//! achsparalleles Bounding-Box-Packen (First-Fit-Decreasing in Regalzeilen,
//! „Shelf Packing"), ohne Rotation. Ein Polygon-Packer (echte Konturen,
//! Drehung) kann das Verfahren später ersetzen — das Rückgabeformat
//! (eine Zielposition je Teil) bleibt dabei stabil.

/// Packt Teile mit den Maßen `sizes` (w, h in mm) in den Rahmen
/// `frame_w`×`frame_h` mit `gap` mm Abstand zwischen den Teilen und zum Rand.
/// Ergebnis je Teil: linke obere Ziel-Ecke, oder `None`, wenn es nicht mehr
/// passt. Die Reihenfolge entspricht der Eingabe.
pub fn nest(sizes: &[(f64, f64)], frame_w: f64, frame_h: f64, gap: f64) -> Vec<Option<(f64, f64)>> {
    if !frame_w.is_finite() || !frame_h.is_finite() || !gap.is_finite() {
        return vec![None; sizes.len()];
    }
    let gap = gap.max(0.0);
    // Nach Höhe absteigend packen (First-Fit-Decreasing) — klassisch gute
    // Regalauslastung; Indizes merken, um die Eingabe-Reihenfolge zu wahren.
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| sizes[b].1.total_cmp(&sizes[a].1));

    let mut out = vec![None; sizes.len()];
    let mut shelf_y = gap; // Oberkante des aktuellen Regals
    let mut shelf_h = 0.0; // Höhe des aktuellen Regals (höchstes Teil darin)
    let mut cursor_x = gap;

    for &i in &order {
        let (w, h) = sizes[i];
        if !w.is_finite()
            || !h.is_finite()
            || w <= 0.0
            || h <= 0.0
            || w + 2.0 * gap > frame_w
            || h + 2.0 * gap > frame_h
        {
            continue; // Teil ist entartet oder größer als der Rahmen
        }
        // Passt es noch in das aktuelle Regal?
        if cursor_x + w + gap > frame_w {
            // Nächstes Regal aufmachen.
            shelf_y += shelf_h + gap;
            shelf_h = 0.0;
            cursor_x = gap;
        }
        if shelf_y + h + gap > frame_h {
            continue; // Bett voll — Teil bleibt ungeplant
        }
        out[i] = Some((cursor_x, shelf_y));
        cursor_x += w + gap;
        shelf_h = shelf_h.max(h);
    }
    out
}

// ── AppState-Anbindung ───────────────────────────────────────────────────────

use crate::state::AppState;

impl AppState {
    /// Packt die selektierten Shapes platzsparend aufs Bett (ein Undo-Punkt).
    /// Teile, die nicht mehr passen, bleiben unverändert liegen.
    pub fn nest_selected(&mut self, gap_mm: f64) {
        if self.selected.len() < 2 {
            return;
        }
        let sel = self.selected.clone();
        let sizes: Vec<(f64, f64)> = sel
            .iter()
            .map(|&i| {
                self.shapes
                    .get(i)
                    .map(|s| {
                        let b = s.bbox();
                        (b.w, b.h)
                    })
                    .unwrap_or((0.0, 0.0))
            })
            .collect();
        let placements = nest(&sizes, self.bed_w_mm, self.bed_h_mm, gap_mm);
        if placements.iter().all(|p| p.is_none()) {
            return;
        }
        self.push_undo();
        for (k, &idx) in sel.iter().enumerate() {
            let Some((tx, ty)) = placements[k] else {
                continue;
            };
            let Some(s) = self.shapes.get_mut(idx) else {
                continue;
            };
            let b = s.bbox();
            s.translate(tx - b.x, ty - b.y);
        }
        self.dirty = true;
    }
}

impl AppState {
    /// Füllt das Bett mit **Kopien** der zuerst selektierten Form (v3-Modus:
    /// „Kopien eines Teils in einen Rahmen packen"). `gap_mm` = Abstand
    /// zwischen den Kopien; es entstehen so viele, wie aufs Bett passen
    /// (inklusive des Originals, das mit einsortiert wird). Ein Undo-Punkt.
    pub fn nest_fill_selected(&mut self, gap_mm: f64) {
        let Some(&first) = self.selected.first() else {
            return;
        };
        let Some(proto) = self.shapes.get(first).cloned() else {
            return;
        };
        let b = proto.bbox();
        if b.w <= 0.0 || b.h <= 0.0 {
            return;
        }
        // Wie viele passen? Kapazität über das Shelf-Packing ermitteln.
        let gap = gap_mm.max(0.0);
        let cols = ((self.bed_w_mm - gap) / (b.w + gap)).floor() as usize;
        let rows = ((self.bed_h_mm - gap) / (b.h + gap)).floor() as usize;
        let count = cols * rows;
        if count < 2 {
            return; // es passt nur das Original — nichts zu tun
        }
        let sizes = vec![(b.w, b.h); count];
        let placements = nest(&sizes, self.bed_w_mm, self.bed_h_mm, gap);

        self.push_undo();
        self.selected.clear();
        let mut placed_first = false;
        for p in placements.into_iter().flatten() {
            let (tx, ty) = p;
            if !placed_first {
                // Das Original an die erste Position schieben.
                if let Some(s) = self.shapes.get_mut(first) {
                    let bb = s.bbox();
                    s.translate(tx - bb.x, ty - bb.y);
                }
                self.selected.push(first);
                placed_first = true;
            } else {
                let mut copy = proto.clone();
                copy.group_id = None;
                copy.text_meta = None;
                let bb = copy.bbox();
                copy.translate(tx - bb.x, ty - bb.y);
                let idx = self.shapes.len();
                self.shapes.push(copy);
                self.selected.push(idx);
            }
        }
        self.dirty = true;
    }

    /// Fügt die 4×2-Untersetzer-Vorlage ein (Referenz: 100 mm, 20 mm Lücke,
    /// zentriert aufs Bett). `round` = runde statt eckige Untersetzer.
    /// Ein Undo-Punkt; die acht Formen sind danach selektiert.
    pub fn insert_coasters(&mut self, round: bool) {
        const COLS: usize = 4;
        const ROWS: usize = 2;
        const SIZE: f64 = 100.0;
        const GAP: f64 = 20.0;
        let total_w = COLS as f64 * SIZE + (COLS as f64 - 1.0) * GAP;
        let total_h = ROWS as f64 * SIZE + (ROWS as f64 - 1.0) * GAP;
        let ox = (self.bed_w_mm - total_w) / 2.0;
        let oy = (self.bed_h_mm - total_h) / 2.0;

        self.push_undo();
        let layer_id = self.layer_for_new_shape();
        self.selected.clear();
        for row in 0..ROWS {
            for col in 0..COLS {
                let x = ox + col as f64 * (SIZE + GAP);
                let y = oy + row as f64 * (SIZE + GAP);
                let geo = if round {
                    crate::geometry::Geo::Ellipse {
                        cx: x + SIZE / 2.0,
                        cy: y + SIZE / 2.0,
                        rx: SIZE / 2.0,
                        ry: SIZE / 2.0,
                    }
                } else {
                    crate::geometry::Geo::Rect {
                        x,
                        y,
                        w: SIZE,
                        h: SIZE,
                    }
                };
                let idx = self.shapes.len();
                self.shapes.push(crate::model::Shape::new(layer_id, geo));
                self.selected.push(idx);
            }
        }
        self.pending_color = None;
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Geo;

    #[test]
    fn packt_ohne_ueberlappung_und_im_rahmen() {
        let sizes = vec![(30.0, 20.0), (30.0, 20.0), (30.0, 20.0), (50.0, 10.0)];
        let placed = nest(&sizes, 100.0, 100.0, 2.0);
        let rects: Vec<(f64, f64, f64, f64)> = placed
            .iter()
            .zip(&sizes)
            .filter_map(|(p, &(w, h))| p.map(|(x, y)| (x, y, w, h)))
            .collect();
        assert_eq!(rects.len(), 4, "alle passen");
        // Im Rahmen …
        for &(x, y, w, h) in &rects {
            assert!(x >= 2.0 - 1e-9 && y >= 2.0 - 1e-9);
            assert!(x + w <= 100.0 + 1e-9 && y + h <= 100.0 + 1e-9);
        }
        // … und paarweise überlappungsfrei.
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let (ax, ay, aw, ah) = rects[i];
                let (bx, by, bw, bh) = rects[j];
                let overlap = ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah;
                assert!(!overlap, "Teile {i} und {j} überlappen");
            }
        }
    }

    #[test]
    fn zu_grosses_teil_bleibt_ungeplant() {
        let placed = nest(&[(200.0, 10.0)], 100.0, 100.0, 2.0);
        assert!(placed[0].is_none());
    }

    #[test]
    fn nicht_finite_werte_bleiben_ungeplant_statt_zu_paniken() {
        let placed = nest(
            &[(f64::NAN, 10.0), (10.0, f64::INFINITY), (10.0, 10.0)],
            100.0,
            100.0,
            2.0,
        );
        assert_eq!(placed[0], None);
        assert_eq!(placed[1], None);
        assert!(placed[2].is_some());
        assert!(nest(&[(10.0, 10.0)], f64::NAN, 100.0, 2.0)[0].is_none());
    }

    #[test]
    fn volles_bett_laesst_rest_liegen() {
        // 100×100-Bett, 60×60-Teile: nur eins passt.
        let placed = nest(&[(60.0, 60.0), (60.0, 60.0)], 100.0, 100.0, 2.0);
        let n = placed.iter().filter(|p| p.is_some()).count();
        assert_eq!(n, 1);
    }

    #[test]
    fn nest_selected_verschiebt_shapes() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 300.0,
            y: 300.0,
            w: 30.0,
            h: 20.0,
        });
        let c = s.layers[0].color;
        s.selected.clear();
        s.activate_color(c);
        s.add_shape(Geo::Rect {
            x: 400.0,
            y: 100.0,
            w: 30.0,
            h: 20.0,
        });
        s.selected = vec![0, 1];
        s.nest_selected(2.0);
        // Beide liegen jetzt oben links gepackt (x klein), nicht mehr verstreut.
        for i in 0..2 {
            let b = s.shapes[i].bbox();
            assert!(
                b.x < 100.0 && b.y < 50.0,
                "Shape {i} gepackt, war {:?}",
                (b.x, b.y)
            );
        }
    }
}
