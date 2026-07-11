//! Shapes anlegen (Teil von `AppState`): primitive/importierte Formen,
//! Bild-Objekte und die Layer-Zuordnung neuer Shapes.

use crate::geometry::{Geo, ImageParams};
use crate::model::{image_layer_color, Layer, LayerMode, Shape};

impl super::AppState {
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
}
