//! Editor-Zustand: Layer, Shapes, Auswahl, Undo/Redo — und das automatische
//! Farbe=Layer-Modell.
//!
//! Kernpunkt (docs/referenz/01-thorburn-analyse.md §1.5): Der Nutzer legt NIE
//! manuell einen Layer an. Er klickt Farben; das System verwaltet Layer
//! automatisch (`activate_color`) und räumt leere Layer weg (`remove_empty_layers`).

use crate::geometry::{Geo, ImageParams};
use crate::model::{image_layer_color, Layer, LayerMode, Shape};

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
    /// `active_layer`.
    ///
    /// Bleibt danach **kein** Layer übrig (alle Objekte gelöscht), bleibt die
    /// Layerliste leer — genau wie bei einem frisch gestarteten Projekt. Der
    /// nächste gezeichnete/importierte Shape legt über `layer_for_new_shape` bzw.
    /// `add_image` wieder einen an. So bleibt nie ein leerer Layer zurück (auch
    /// kein leerer Image-Layer, ADR 0004).
    pub fn remove_empty_layers(&mut self) {
        let used: std::collections::HashSet<usize> =
            self.shapes.iter().map(|s| s.layer_id).collect();

        let keep: Vec<usize> = (0..self.layers.len())
            .filter(|i| used.contains(i))
            .collect();
        if keep.len() == self.layers.len() {
            return; // alle Layer sind belegt — nichts zu tun
        }

        let mut remap = vec![0usize; self.layers.len()];
        for (new_idx, &old_idx) in keep.iter().enumerate() {
            remap[old_idx] = new_idx;
        }
        self.layers = keep.iter().map(|&i| self.layers[i].clone()).collect();
        for shape in &mut self.shapes {
            shape.layer_id = remap[shape.layer_id];
        }
        // active_layer auf einen gültigen Index klemmen (0 bei leerer Liste).
        self.active_layer = if used.contains(&self.active_layer) {
            remap[self.active_layer]
        } else {
            0
        };
    }

    /// Verschiebt einen Layer von Position `from` an Position `to` (ADR 0005 §0).
    ///
    /// Die **Reihenfolge der `layers`-Liste IST die Brenn-Reihenfolge** (Index 0
    /// zuerst). Weil Shapes ihren Layer per **Index** (`layer_id`) referenzieren,
    /// werden hier in **derselben Operation alle `shape.layer_id`** und der
    /// `active_layer` remappt, sodass jede Form nach dem Verschieben auf denselben
    /// Layer wie vorher zeigt. Legt einen Undo-Punkt an.
    ///
    /// Ungültige Indizes oder `from == to` sind No-Ops (kein Undo-Punkt).
    pub fn move_layer(&mut self, from: usize, to: usize) {
        let n = self.layers.len();
        if from >= n || to >= n || from == to {
            return;
        }
        self.push_undo();

        // Layer physisch umsortieren: entnehmen und an Zielposition einfügen.
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);

        // Permutations-Remap alt_idx -> neu_idx aufbauen. Nur die Indizes im
        // Bereich [min(from,to), max(from,to)] verschieben sich um eins; alle
        // anderen bleiben. Wir bauen die Abbildung generisch aus der Bewegung.
        let remap = |old: usize| -> usize {
            if old == from {
                to
            } else if from < to {
                // Layer zwischen from+1..=to rücken um eins nach vorne.
                if old > from && old <= to {
                    old - 1
                } else {
                    old
                }
            } else {
                // from > to: Layer zwischen to..from rücken um eins nach hinten.
                if old >= to && old < from {
                    old + 1
                } else {
                    old
                }
            }
        };

        for shape in &mut self.shapes {
            shape.layer_id = remap(shape.layer_id);
        }
        self.active_layer = remap(self.active_layer);
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

    /// Fügt mehrere Polylinien als **einen** Undo-Punkt hinzu und selektiert
    /// sie (Trace-Ergebnis, Vektor-Import, Text→Pfad). Layer wie bei
    /// `add_shape` (pending_color bzw. aktiver Layer).
    pub fn add_polylines(&mut self, contours: Vec<(Vec<crate::geometry::Pt>, bool)>) -> Vec<usize> {
        let contours: Vec<_> = contours
            .into_iter()
            .filter(|(pts, _)| pts.len() >= 2)
            .collect();
        if contours.is_empty() {
            return Vec::new();
        }
        self.push_undo();
        let layer_id = self.layer_for_new_shape();
        self.selected.clear();
        let mut idxs = Vec::with_capacity(contours.len());
        for (pts, closed) in contours {
            let idx = self.shapes.len();
            self.shapes
                .push(Shape::new(layer_id, Geo::Polyline { pts, closed }));
            self.selected.push(idx);
            idxs.push(idx);
        }
        self.pending_color = None;
        idxs
    }

    /// Fügt ein importiertes Bild ein (ADR 0004): legt **immer einen eigenen
    /// Image-Layer** mit katalogfremder Kennfarbe an (jedes Bild = eigener Layer,
    /// nie den aktiven wiederverwenden) und platziert das Bild-Shape darauf. Gibt
    /// den Shape-Index zurück. `asset` ist die Store-ID, `w`/`h` die Zielgröße in
    /// mm, `x`/`y` die linke obere Ecke.
    pub fn add_image(&mut self, asset: String, x: f64, y: f64, w: f64, h: f64) -> usize {
        self.push_undo();
        // Eigener Layer mit garantiert katalogfremder Farbe. seed = Anzahl der
        // bereits vorhandenen Image-Layer, damit sich die Farben streuen.
        let seed = self
            .layers
            .iter()
            .filter(|l| l.mode == LayerMode::Image)
            .count() as u32;
        let layer_id = self.layers.len();
        let mut layer = Layer::with_color(layer_id, image_layer_color(seed));
        layer.mode = LayerMode::Image;
        layer.name = format!("Bild {}", seed + 1);
        self.layers.push(layer);

        let geo = Geo::Image {
            asset,
            x,
            y,
            w,
            h,
            params: ImageParams::default(),
        };
        self.shapes.push(Shape::new(layer_id, geo));
        let idx = self.shapes.len() - 1;
        self.selected = vec![idx];
        self.active_layer = layer_id;
        self.pending_color = None;
        idx
    }

    /// Bestimmt den Layer für eine neu **gezeichnete** (Vektor-)Form:
    /// pending_color → passenden Layer finden/anlegen; sonst der aktive Layer.
    ///
    /// Ein **Image-Layer ist nie Ziel** einer gezeichneten Form (ADR 0004: ein
    /// Image-Layer trägt genau ein Bild). Ist der aktive Layer ein Image-Layer
    /// (z. B. weil gerade ein Bild markiert war), wird der erste normale Layer
    /// genutzt bzw. ein frischer angelegt.
    pub(crate) fn layer_for_new_shape(&mut self) -> usize {
        if let Some(color) = self.pending_color {
            return self.find_or_create_layer(color);
        }
        // Aktiver Layer, falls er ein normaler (Nicht-Image-)Layer ist.
        if let Some(l) = self.layers.get(self.active_layer) {
            if l.mode != LayerMode::Image {
                return self.active_layer;
            }
        }
        // Sonst: ersten normalen Layer suchen …
        if let Some(idx) = self.layers.iter().position(|l| l.mode != LayerMode::Image) {
            return idx;
        }
        // … oder einen neuen anlegen (nur Image-Layer bzw. gar keine vorhanden).
        let idx = self.layers.len();
        self.layers.push(Layer::new(idx));
        idx
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

    // ---- Text-Blöcke (Text→Pfad, editierbar) --------------------------------

    /// Fügt einen Text-Block ein: alle Konturen als EINE Gruppe (verhält sich
    /// als Einheit), die Quelldaten (`TextMeta`) am ersten Shape für späteres
    /// Editieren per Doppelklick. Ein Undo-Punkt.
    pub fn add_text_block(
        &mut self,
        contours: Vec<(Vec<crate::geometry::Pt>, bool)>,
        meta: crate::model::TextMeta,
    ) -> Vec<usize> {
        let idxs = self.add_polylines(contours);
        if idxs.is_empty() {
            return idxs;
        }
        let gid = self
            .shapes
            .iter()
            .filter_map(|s| s.group_id)
            .max()
            .unwrap_or(0)
            + 1;
        for &i in &idxs {
            if let Some(s) = self.shapes.get_mut(i) {
                s.group_id = Some(gid);
            }
        }
        if let Some(s) = self.shapes.get_mut(idxs[0]) {
            s.text_meta = Some(meta);
        }
        idxs
    }

    /// Ersetzt den Text-Block, zu dem `idx` gehört (Doppelklick-Edit): die
    /// alte Gruppe wird entfernt, die neuen Konturen erscheinen an derselben
    /// Position (Anker = alte linke Oberkante) auf demselben Layer.
    /// Ein Undo-Punkt.
    pub fn replace_text_block(
        &mut self,
        idx: usize,
        contours: Vec<(Vec<crate::geometry::Pt>, bool)>,
        meta: crate::model::TextMeta,
    ) {
        let Some(anchor_shape) = self.shapes.get(idx) else {
            return;
        };
        let gid = anchor_shape.group_id;
        let layer_id = anchor_shape.layer_id;
        // Mitglieder des Blocks (bei fehlender Gruppe: nur das eine Shape).
        let members: Vec<usize> = match gid {
            Some(g) => (0..self.shapes.len())
                .filter(|&i| self.shapes[i].group_id == Some(g))
                .collect(),
            None => vec![idx],
        };
        // Alte Position (linke Oberkante des Blocks).
        let (mut ox, mut oy) = (f64::MAX, f64::MAX);
        for &i in &members {
            let b = self.shapes[i].bbox();
            ox = ox.min(b.x);
            oy = oy.min(b.y);
        }
        // Neue Konturen auf den alten Anker verschieben.
        let (mut nx, mut ny) = (f64::MAX, f64::MAX);
        for (pts, _) in &contours {
            for &(x, y) in pts {
                nx = nx.min(x);
                ny = ny.min(y);
            }
        }
        if nx == f64::MAX {
            return;
        }
        let placed: Vec<(Vec<crate::geometry::Pt>, bool)> = contours
            .into_iter()
            .map(|(pts, closed)| {
                (
                    pts.into_iter()
                        .map(|(x, y)| (x - nx + ox, y - ny + oy))
                        .collect(),
                    closed,
                )
            })
            .collect();

        self.push_undo();
        // Alte Mitglieder entfernen (absteigend), dann neu einfügen.
        let mut rm = members.clone();
        rm.sort_unstable();
        for &i in rm.iter().rev() {
            self.shapes.remove(i);
        }
        let new_gid = self
            .shapes
            .iter()
            .filter_map(|s| s.group_id)
            .max()
            .unwrap_or(0)
            + 1;
        self.selected.clear();
        let mut first = None;
        for (pts, closed) in placed {
            let i = self.shapes.len();
            let mut sh = Shape::new(layer_id, Geo::Polyline { pts, closed });
            sh.group_id = Some(new_gid);
            self.shapes.push(sh);
            self.selected.push(i);
            first.get_or_insert(i);
        }
        if let Some(f) = first {
            self.shapes[f].text_meta = Some(meta);
        }
        self.remove_empty_layers();
        self.dirty = true;
    }

    // ---- Gruppen (group_id) ------------------------------------------------

    /// Erweitert die Auswahl auf ganze Gruppen: ist ein Gruppenmitglied
    /// selektiert, werden alle Mitglieder selektiert. Nach jeder
    /// Auswahländerung aufrufen — so verhält sich eine Gruppe als Einheit.
    pub fn expand_selection_to_groups(&mut self) {
        let mut gids: Vec<u32> = self
            .selected
            .iter()
            .filter_map(|&i| self.shapes.get(i).and_then(|s| s.group_id))
            .collect();
        gids.sort_unstable();
        gids.dedup();
        if gids.is_empty() {
            return;
        }
        for (i, s) in self.shapes.iter().enumerate() {
            if let Some(g) = s.group_id {
                if gids.contains(&g) && !self.selected.contains(&i) {
                    self.selected.push(i);
                }
            }
        }
    }

    /// Gruppiert die Auswahl (ein Undo-Punkt): alle selektierten Shapes
    /// bekommen dieselbe neue Gruppen-ID (bestehende Gruppen gehen darin auf).
    pub fn group_selected(&mut self) {
        if self.selected.len() < 2 {
            return;
        }
        self.push_undo();
        let next = self
            .shapes
            .iter()
            .filter_map(|s| s.group_id)
            .max()
            .unwrap_or(0)
            + 1;
        let sel = self.selected.clone();
        for idx in sel {
            if let Some(s) = self.shapes.get_mut(idx) {
                s.group_id = Some(next);
            }
        }
        self.dirty = true;
    }

    /// Löst die Gruppierung der Auswahl (ein Undo-Punkt).
    pub fn ungroup_selected(&mut self) {
        let has_group = self
            .selected
            .iter()
            .any(|&i| self.shapes.get(i).is_some_and(|s| s.group_id.is_some()));
        if !has_group {
            return;
        }
        self.push_undo();
        let sel = self.selected.clone();
        for idx in sel {
            if let Some(s) = self.shapes.get_mut(idx) {
                s.group_id = None;
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
