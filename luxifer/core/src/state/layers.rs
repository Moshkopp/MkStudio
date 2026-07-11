//! Farbe=Layer-Modell und Layer-Reihenfolge (Teil von `AppState`).
//! Der Nutzer legt nie manuell Layer an — Farbe klicken verwaltet sie.

use crate::model::Layer;

impl super::AppState {
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
    pub(super) fn find_or_create_layer(&mut self, color: [u8; 3]) -> usize {
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
}
