//! Editor-Zustand: Layer, Shapes, Auswahl, Undo/Redo — und das automatische
//! Farbe=Layer-Modell.
//!
//! Kernpunkt (docs/referenz/01-thorburn-analyse.md §1.5): Der Nutzer legt NIE
//! manuell einen Layer an. Er klickt Farben; das System verwaltet Layer
//! automatisch (`activate_color`) und räumt leere Layer weg (`remove_empty_layers`).

use crate::geometry::Geo;
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

    // ---- Farbe = Layer (automatisch) --------------------------------------

    /// Farbe klicken (Farbpalette). Kernlogik des Farbe=Layer-Modells.
    ///
    /// - Sind Shapes selektiert: Layer mit dieser Farbe suchen, sonst neu
    ///   anlegen; alle selektierten Shapes in diesen Layer verschieben; leere
    ///   Layer entfernen.
    /// - Ist nichts selektiert: existiert der Farb-Layer → aktiv setzen; und in
    ///   jedem Fall die Farbe als `pending_color` merken (der Layer entsteht
    ///   sonst erst beim nächsten gezeichneten Shape).
    pub fn activate_color(&mut self, color: [u8; 3]) {
        if !self.selected.is_empty() {
            self.push_undo();
            let layer_id = self.find_or_create_layer(color);
            self.active_layer = layer_id;
            let sel = self.selected.clone();
            for idx in sel {
                if let Some(shape) = self.shapes.get_mut(idx) {
                    shape.layer_id = layer_id;
                }
            }
            self.remove_empty_layers();
        } else {
            if let Some(idx) = self.layers.iter().position(|l| l.color == color) {
                self.active_layer = idx;
            }
            self.pending_color = Some(color);
        }
    }

    /// Index eines Layers mit der Farbe; legt ihn an, falls nicht vorhanden.
    fn find_or_create_layer(&mut self, color: [u8; 3]) -> usize {
        if let Some(idx) = self.layers.iter().position(|l| l.color == color) {
            idx
        } else {
            let idx = self.layers.len();
            self.layers.push(Layer::with_color(idx, color));
            idx
        }
    }

    /// Entfernt Layer ohne zugehörige Shapes und remappt alle `layer_id` sowie
    /// `active_layer`. Mindestens ein Layer bleibt erhalten.
    pub fn remove_empty_layers(&mut self) {
        if self.layers.len() <= 1 {
            return;
        }
        let used: std::collections::HashSet<usize> =
            self.shapes.iter().map(|s| s.layer_id).collect();

        let keep: Vec<usize> = (0..self.layers.len())
            .filter(|i| used.contains(i))
            .collect();
        if keep.is_empty() || keep.len() == self.layers.len() {
            return; // nichts zu tun (oder alle leer → behalten wie es ist)
        }

        let mut remap = vec![0usize; self.layers.len()];
        for (new_idx, &old_idx) in keep.iter().enumerate() {
            remap[old_idx] = new_idx;
        }
        self.layers = keep.iter().map(|&i| self.layers[i].clone()).collect();
        for shape in &mut self.shapes {
            shape.layer_id = remap[shape.layer_id];
        }
        self.active_layer = if used.contains(&self.active_layer) {
            remap[self.active_layer]
        } else {
            0
        };
    }

    // ---- Shapes anlegen ---------------------------------------------------

    /// Fügt eine gezeichnete Geometrie als neue Shape hinzu. Die Farbe/der Layer
    /// ergibt sich aus `pending_color` (bzw. dem aktiven Layer) — hier entsteht
    /// bei Bedarf der Layer. Legt einen Undo-Punkt an und selektiert die neue
    /// Form. Gibt den Shape-Index zurück.
    pub fn add_shape(&mut self, geo: Geo) -> usize {
        self.push_undo();
        let layer_id = self.layer_for_new_shape();
        let shape = Shape::new(layer_id, geo);
        self.shapes.push(shape);
        let idx = self.shapes.len() - 1;
        self.selected = vec![idx];
        self.pending_color = None;
        idx
    }

    /// Bestimmt den Layer für eine neu gezeichnete Form:
    /// pending_color → passenden Layer finden/anlegen; sonst aktiver Layer;
    /// existiert gar kein Layer → Layer 0 anlegen.
    fn layer_for_new_shape(&mut self) -> usize {
        if let Some(color) = self.pending_color {
            return self.find_or_create_layer(color);
        }
        if self.layers.is_empty() {
            self.layers.push(Layer::new(0));
            return 0;
        }
        self.active_layer.min(self.layers.len() - 1)
    }

    // ---- Auswahl / Verschieben --------------------------------------------

    /// Oberstes getroffenes Shape (spätere Shapes liegen oben). Überspringt
    /// unsichtbare/gesperrte Layer. Gibt den Shape-Index zurück.
    pub fn hit_test(&self, px: f64, py: f64, tol: f64) -> Option<usize> {
        for i in (0..self.shapes.len()).rev() {
            let s = &self.shapes[i];
            if let Some(l) = self.layers.get(s.layer_id) {
                if !l.visible || l.locked {
                    continue;
                }
            }
            if s.hit_test(px, py, tol) {
                return Some(i);
            }
        }
        None
    }

    /// Verschiebt alle selektierten Shapes um (dx, dy). Kein eigener Undo-Punkt
    /// (der Aufrufer setzt einen zu Drag-Beginn).
    pub fn translate_selected(&mut self, dx: f64, dy: f64) {
        for &idx in &self.selected {
            if let Some(s) = self.shapes.get_mut(idx) {
                s.geo.translate(dx, dy);
            }
        }
        self.dirty = true;
    }

    /// Löscht die selektierten Shapes (ein Undo-Punkt) und räumt leere Layer weg.
    pub fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let mut sel = self.selected.clone();
        sel.sort_unstable();
        sel.dedup();
        for &idx in sel.iter().rev() {
            if idx < self.shapes.len() {
                self.shapes.remove(idx);
            }
        }
        self.selected.clear();
        self.remove_empty_layers();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Mindestens ein Layer bleibt (remove_empty_layers behält bei <=1 bzw. alle-leer).
        assert!(!s.layers.is_empty());
    }

    #[test]
    fn hit_test_ueberspringt_gesperrte_layer() {
        let mut s = AppState::new();
        s.add_shape(rect());
        assert_eq!(s.hit_test(5.0, 5.0, 0.0), Some(0));
        s.layers[0].locked = true;
        assert_eq!(s.hit_test(5.0, 5.0, 0.0), None);
    }
}
