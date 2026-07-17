//! Bestehende Shapes bearbeiten (Teil von `AppState`): Auswahl/Hit-Test,
//! Verschieben, Text-Blöcke, Gruppen (group_id), Löschen.

use crate::geometry::Geo;
use crate::model::Shape;

impl super::AppState {
    // ---- Auswahl / Verschieben --------------------------------------------

    /// Oberstes getroffenes Shape (spätere Shapes liegen oben). Überspringt
    /// unsichtbare/gesperrte Layer. Gibt den Shape-Index zurück.
    pub fn hit_test(&self, px: f64, py: f64, tol: f64) -> Option<usize> {
        for i in (0..self.shapes.len()).rev() {
            let s = &self.shapes[i];
            if let Some(l) = self.layers.get(s.layer_id) {
                if !l.visible || l.locked || (s.fill_only && !l.mode.is_filled()) {
                    continue;
                }
            }
            // Grober, billiger Vorfilter vor dem exakten Segment-Hit-Test.
            // Große DXFs enthalten zehntausende Segmente über viele Konturen;
            // ohne BBox-Test wird für jedes davon der Punkt-Segment-Abstand
            // berechnet, obwohl fast alle Shapes weit vom Klick entfernt sind.
            let b = self.shape_bbox_cached(i)?;
            if px < b.x - tol || px > b.x + b.w + tol || py < b.y - tol || py > b.y + b.h + tol {
                continue;
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
                s.translate(dx, dy);
            }
        }
        // Verschobene Shapes → gecachte Bounds sind ungültig. Ohne das driftet
        // die Auswahl-BBox (aus dem Cache) vom Shape weg.
        self.invalidate_shape_bounds();
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
        let new_fill_gid = self
            .shapes
            .iter()
            .filter_map(|shape| shape.fill_group_id)
            .max()
            .unwrap_or(0)
            + 1;
        self.selected.clear();
        let mut first = None;
        for (pts, closed) in placed {
            let i = self.shapes.len();
            let mut sh = Shape::new(layer_id, Geo::Polyline { pts, closed });
            sh.group_id = Some(new_gid);
            sh.fill_group_id = Some(new_fill_gid);
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
