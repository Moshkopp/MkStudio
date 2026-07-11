//! Editor-Zustand: Layer, Shapes, Auswahl, Undo/Redo — und das automatische
//! Farbe=Layer-Modell.
//!
//! Kernpunkt (docs/referenz/01-thorburn-analyse.md §1.5): Der Nutzer legt NIE
//! manuell einen Layer an. Er klickt Farben; das System verwaltet Layer
//! automatisch (`activate_color`) und räumt leere Layer weg (`remove_empty_layers`).

use crate::model::{Layer, Shape};

/// Standard-Bettgröße in mm.
pub const DEFAULT_BED_W: f64 = 600.0;
pub const DEFAULT_BED_H: f64 = 400.0;

/// Ein Undo-Schnappschuss des relevanten Zustands.
#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    layers: Vec<Layer>,
    active_layer: usize,
    shapes: Vec<Shape>,
    selected: Vec<usize>,
    pending_color: Option<[u8; 3]>,
}

/// Der gesamte Editor-Zustand. UI-frei.
#[derive(Debug, Clone)]
pub struct AppState {
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    pub shapes: Vec<Shape>,
    /// Indizes der selektierten Shapes.
    pub selected: Vec<usize>,
    pub bed_w_mm: f64,
    pub bed_h_mm: f64,
    /// Nächste Zeichenfarbe — der Layer entsteht erst beim nächsten Shape.
    pub pending_color: Option<[u8; 3]>,
    pub dirty: bool,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            active_layer: 0,
            shapes: Vec::new(),
            selected: Vec::new(),
            bed_w_mm: DEFAULT_BED_W,
            bed_h_mm: DEFAULT_BED_H,
            pending_color: None,
            dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- Undo / Redo (Snapshot-basiert) -----------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            layers: self.layers.clone(),
            active_layer: self.active_layer,
            shapes: self.shapes.clone(),
            selected: self.selected.clone(),
            pending_color: self.pending_color,
        }
    }

    fn restore(&mut self, s: Snapshot) {
        self.layers = s.layers;
        self.active_layer = s.active_layer;
        self.shapes = s.shapes;
        self.selected = s.selected;
        self.pending_color = s.pending_color;
    }

    /// Vor jeder mutierenden Aktion aufrufen: sichert den aktuellen Zustand.
    pub fn push_undo(&mut self) {
        self.undo_stack.push(self.snapshot());
        self.redo_stack.clear();
        self.dirty = true;
    }

    /// Nach erfolgreichem Speichern: der Zustand gilt als gesichert.
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.snapshot());
            self.restore(prev);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.snapshot());
            self.restore(next);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Verwirft den letzten Undo-Schnappschuss, wenn sich nichts geändert hat.
    /// (Nach einem `push_undo`, das doch keine Änderung nach sich zog.)
    pub fn discard_last_undo_if_no_change(&mut self) {
        if let Some(last) = self.undo_stack.last() {
            if last.layers == self.layers
                && last.shapes == self.shapes
                && last.active_layer == self.active_layer
            {
                self.undo_stack.pop();
            }
        }
    }
}

mod edit;
mod layers;
mod shapes;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Geo;
    use crate::model::LayerMode;

    fn rect() -> Geo {
        Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        }
    }

    #[test]
    fn add_shape_ohne_layer_legt_layer_an() {
        let mut s = AppState::new();
        assert!(s.layers.is_empty());
        s.add_shape(rect());
        assert_eq!(s.layers.len(), 1);
        assert_eq!(s.shapes.len(), 1);
        assert_eq!(s.shapes[0].layer_id, 0);
        assert_eq!(s.selected, vec![0]);
    }

    #[test]
    fn pending_color_ohne_auswahl_erstellt_keinen_leeren_layer() {
        let mut s = AppState::new();
        s.activate_color([0x3B, 0x82, 0xF6]); // blau, nichts selektiert
        assert!(s.layers.is_empty(), "kein leerer Layer");
        assert_eq!(s.pending_color, Some([0x3B, 0x82, 0xF6]));
        // Erst das nächste Shape legt den blauen Layer an.
        s.add_shape(rect());
        assert_eq!(s.layers.len(), 1);
        assert_eq!(s.layers[0].color, [0x3B, 0x82, 0xF6]);
    }

    #[test]
    fn farbe_auf_selektiertes_shape_verschiebt_in_farb_layer() {
        let mut s = AppState::new();
        s.add_shape(rect()); // Layer 0 (rot), Shape selektiert
        let first_color = s.layers[0].color;
        assert_eq!(s.shapes[0].layer_id, 0);

        // Andere Farbe klicken → neuer Layer, Shape wandert rüber, alter (leerer) weg.
        let blue = [0x3B, 0x82, 0xF6];
        assert_ne!(first_color, blue);
        s.activate_color(blue);
        assert_eq!(s.layers.len(), 1, "alter leerer Layer entfernt");
        assert_eq!(s.layers[0].color, blue);
        assert_eq!(s.shapes[0].layer_id, 0);
    }

    #[test]
    fn zwei_shapes_verschiedene_farben_zwei_layer() {
        let mut s = AppState::new();
        s.add_shape(rect()); // rot, Layer 0
        s.selected.clear();
        s.activate_color([0x3B, 0x82, 0xF6]); // pending blau
        s.add_shape(Geo::Rect {
            x: 20.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        }); // blau, Layer 1
        assert_eq!(s.layers.len(), 2);
        assert_ne!(s.shapes[0].layer_id, s.shapes[1].layer_id);
    }

    #[test]
    fn gleiche_farbe_zweimal_teilt_layer() {
        let mut s = AppState::new();
        s.add_shape(rect());
        let c = s.layers[0].color;
        s.selected.clear();
        s.activate_color(c); // pending gleiche Farbe
        s.add_shape(Geo::Rect {
            x: 20.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        assert_eq!(s.layers.len(), 1, "gleiche Farbe = ein Layer");
        assert_eq!(s.shapes[0].layer_id, s.shapes[1].layer_id);
    }

    #[test]
    fn undo_redo_stellt_zustand_wieder_her() {
        let mut s = AppState::new();
        s.add_shape(rect());
        assert_eq!(s.shapes.len(), 1);
        assert!(s.undo());
        assert_eq!(s.shapes.len(), 0);
        assert!(s.redo());
        assert_eq!(s.shapes.len(), 1);
    }

    #[test]
    fn delete_selected_entfernt_und_raeumt_layer() {
        let mut s = AppState::new();
        s.add_shape(rect());
        s.delete_selected();
        assert_eq!(s.shapes.len(), 0);
        // Kein Objekt mehr → keine Layer (wie frisches Projekt). Der nächste
        // gezeichnete Shape legt wieder einen an.
        assert!(s.layers.is_empty(), "leeres Projekt hat keine Layer");
        // Weiterzeichnen funktioniert trotzdem: neuer Shape → neuer Layer.
        s.add_shape(rect());
        assert_eq!(s.layers.len(), 1);
    }

    #[test]
    fn hit_test_ueberspringt_gesperrte_layer() {
        let mut s = AppState::new();
        s.add_shape(rect());
        assert_eq!(s.hit_test(5.0, 5.0, 0.0), Some(0));
        s.layers[0].locked = true;
        assert_eq!(s.hit_test(5.0, 5.0, 0.0), None);
    }

    #[test]
    fn add_image_legt_eigenen_image_layer_an() {
        let mut s = AppState::new();
        // Erst eine normale Form (belegt Layer 0).
        s.add_shape(rect());
        let n_layers_vorher = s.layers.len();

        let idx = s.add_image("asset-abc".into(), 10.0, 20.0, 100.0, 80.0);
        // Neuer, eigener Layer im Image-Modus.
        assert_eq!(s.layers.len(), n_layers_vorher + 1);
        let li = s.shapes[idx].layer_id;
        assert_eq!(s.layers[li].mode, LayerMode::Image);
        // Katalogfremde Farbe.
        assert!(!crate::model::SWATCH_COLORS.contains(&s.layers[li].color));
        // Geometrie stimmt.
        if let Geo::Image {
            asset, x, y, w, h, ..
        } = &s.shapes[idx].geo
        {
            assert_eq!(asset, "asset-abc");
            assert_eq!((*x, *y, *w, *h), (10.0, 20.0, 100.0, 80.0));
        } else {
            panic!("erwarte Geo::Image");
        }

        // Zweites Bild → wieder eigener Layer, andere Farbe.
        let idx2 = s.add_image("asset-def".into(), 0.0, 0.0, 50.0, 50.0);
        let li2 = s.shapes[idx2].layer_id;
        assert_ne!(li, li2, "jedes Bild eigener Layer");
        assert_ne!(s.layers[li].color, s.layers[li2].color);
    }

    #[test]
    fn rect_nach_bild_landet_nicht_auf_image_layer() {
        // Bild importieren (Image-Layer wird aktiv + selektiert), dann ein Rect
        // zeichnen. Das Rect darf NICHT auf dem Image-Layer landen und die
        // Auswahl muss auf das neue Rect wechseln (nicht das Bild behalten).
        let mut s = AppState::new();
        let img_idx = s.add_image("asset-y".into(), 0.0, 0.0, 10.0, 10.0);
        let img_layer = s.shapes[img_idx].layer_id;
        assert_eq!(s.selected, vec![img_idx]);

        let rect_idx = s.add_shape(rect());
        let rect_layer = s.shapes[rect_idx].layer_id;
        assert_ne!(rect_layer, img_layer, "Rect nicht auf dem Image-Layer");
        assert_ne!(s.layers[rect_layer].mode, LayerMode::Image);
        // Auswahl nur noch das neue Rect.
        assert_eq!(s.selected, vec![rect_idx]);
    }

    #[test]
    fn einziges_bild_loeschen_entfernt_image_layer() {
        // Nur ein Bild, sonst nichts. Nach dem Löschen darf KEIN (leerer)
        // Image-Layer zurückbleiben.
        let mut s = AppState::new();
        let img_idx = s.add_image("asset-solo".into(), 0.0, 0.0, 10.0, 10.0);
        assert_eq!(s.layers.len(), 1);
        assert_eq!(s.layers[0].mode, LayerMode::Image);

        s.selected = vec![img_idx];
        s.delete_selected();

        assert_eq!(s.shapes.len(), 0, "kein Shape mehr");
        // Kein zurückgebliebener Image-Layer.
        assert!(
            !s.layers.iter().any(|l| l.mode == LayerMode::Image),
            "leerer Image-Layer muss entfernt sein"
        );
    }

    #[test]
    fn bild_loeschen_entfernt_leeren_image_layer() {
        let mut s = AppState::new();
        s.add_shape(rect()); // Layer 0 (normale Form)
        let img_idx = s.add_image("asset-x".into(), 0.0, 0.0, 10.0, 10.0); // Layer 1 (Bild)
        assert_eq!(s.layers.len(), 2);

        // Bild selektieren und löschen.
        s.selected = vec![img_idx];
        s.delete_selected();

        // Der leere Bild-Layer muss weg sein; der Form-Layer bleibt.
        assert_eq!(s.shapes.len(), 1, "nur die Form bleibt");
        assert_eq!(s.layers.len(), 1, "leerer Bild-Layer entfernt");
        assert_ne!(
            s.layers[0].mode,
            LayerMode::Image,
            "verbleibender Layer ist der Form-Layer"
        );
    }

    // ---- move_layer (Layer-Reihenfolge, ADR 0005 §0) ----------------------

    /// Baut drei Layer (0,1,2) mit je genau einer Form, jeweils andere Farbe.
    /// Rückgabe-Reihenfolge der Layer entspricht der Anlage-Reihenfolge.
    fn state_three_layers() -> AppState {
        let mut s = AppState::new();
        let colors = [[10, 0, 0], [0, 20, 0], [0, 0, 30]];
        for (i, c) in colors.iter().enumerate() {
            s.selected.clear();
            s.activate_color(*c); // pending Farbe
            s.add_shape(Geo::Rect {
                x: (i * 20) as f64,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            });
        }
        assert_eq!(s.layers.len(), 3);
        // Shape i liegt auf Layer i.
        for i in 0..3 {
            assert_eq!(s.shapes[i].layer_id, i);
        }
        s
    }

    #[test]
    fn move_layer_aendert_reihenfolge_und_haelt_shape_zuordnung() {
        let mut s = state_three_layers();
        let farbe_von =
            |s: &AppState, shape_idx: usize| s.layers[s.shapes[shape_idx].layer_id].color;
        // Vor dem Verschieben: Shape i hat Farbe der Anlage.
        let f0 = farbe_von(&s, 0);
        let f2 = farbe_von(&s, 2);

        // Letzten Layer ganz nach vorne holen (typisch: Schneiden zuletzt → vorziehen).
        s.move_layer(2, 0);

        // Reihenfolge ist jetzt [alt2, alt0, alt1].
        assert_eq!(s.layers[0].color, [0, 0, 30]);
        assert_eq!(s.layers[1].color, [10, 0, 0]);
        assert_eq!(s.layers[2].color, [0, 20, 0]);

        // Entscheidend: Jede Form zeigt weiterhin auf IHREN Layer (gleiche Farbe).
        assert_eq!(farbe_von(&s, 0), f0, "Shape 0 folgt seinem Layer");
        assert_eq!(farbe_von(&s, 2), f2, "Shape 2 folgt seinem Layer");
    }

    #[test]
    fn move_layer_vorwaerts_remappt_korrekt() {
        let mut s = state_three_layers();
        // Ersten Layer nach hinten schieben: [0,1,2] -> [1,2,0].
        s.move_layer(0, 2);
        assert_eq!(s.layers[0].color, [0, 20, 0]);
        assert_eq!(s.layers[1].color, [0, 0, 30]);
        assert_eq!(s.layers[2].color, [10, 0, 0]);
        // Shape, das auf dem verschobenen Layer lag, zeigt jetzt auf Index 2.
        assert_eq!(s.shapes[0].layer_id, 2);
        // Die anderen beiden sind je einen nach vorne gerückt.
        assert_eq!(s.shapes[1].layer_id, 0);
        assert_eq!(s.shapes[2].layer_id, 1);
    }

    #[test]
    fn move_layer_haelt_active_layer_stabil() {
        let mut s = state_three_layers();
        s.active_layer = 2; // der hinterste Layer ist aktiv
        let aktive_farbe = s.layers[s.active_layer].color;
        s.move_layer(2, 0); // ihn nach vorne holen
                            // active_layer zeigt weiter auf denselben Layer (jetzt Index 0).
        assert_eq!(s.layers[s.active_layer].color, aktive_farbe);
        assert_eq!(s.active_layer, 0);
    }

    #[test]
    fn move_layer_noop_bei_gleichem_index_oder_ungueltig() {
        let mut s = state_three_layers();
        let before = s.layers.clone();
        // Zählt, wie viele Undo-Schritte aktuell möglich sind.
        let undo_tiefe = |s: &AppState| {
            let mut c = s.clone();
            let mut n = 0;
            while c.undo() {
                n += 1;
            }
            n
        };
        let tiefe_vorher = undo_tiefe(&s);

        s.move_layer(1, 1); // gleich → No-Op
        assert_eq!(s.layers, before);
        s.move_layer(0, 9); // Ziel außerhalb → No-Op
        assert_eq!(s.layers, before);
        s.move_layer(5, 0); // Quelle außerhalb → No-Op
        assert_eq!(s.layers, before);

        // Kein No-Op darf einen zusätzlichen Undo-Punkt angelegt haben.
        assert_eq!(
            undo_tiefe(&s),
            tiefe_vorher,
            "No-Op legt keinen Undo-Punkt an"
        );
    }

    #[test]
    fn move_layer_ist_per_undo_zuruecknehmbar() {
        let mut s = state_three_layers();
        let before = s.layers.clone();
        let shapes_before = s.shapes.clone();
        s.move_layer(2, 0);
        assert_ne!(s.layers, before);
        assert!(s.undo(), "move_layer ist eine mutierende Aktion mit Undo");
        assert_eq!(s.layers, before, "alte Reihenfolge wiederhergestellt");
        assert_eq!(s.shapes, shapes_before, "layer_id der Shapes zurückgesetzt");
    }
}
