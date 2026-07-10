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
    let gap = gap.max(0.0);
    // Nach Höhe absteigend packen (First-Fit-Decreasing) — klassisch gute
    // Regalauslastung; Indizes merken, um die Eingabe-Reihenfolge zu wahren.
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| sizes[b].1.partial_cmp(&sizes[a].1).unwrap());

    let mut out = vec![None; sizes.len()];
    let mut shelf_y = gap; // Oberkante des aktuellen Regals
    let mut shelf_h = 0.0; // Höhe des aktuellen Regals (höchstes Teil darin)
    let mut cursor_x = gap;

    for &i in &order {
        let (w, h) = sizes[i];
        if w <= 0.0 || h <= 0.0 || w + 2.0 * gap > frame_w || h + 2.0 * gap > frame_h {
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
            s.geo.translate(tx - b.x, ty - b.y);
        }
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
