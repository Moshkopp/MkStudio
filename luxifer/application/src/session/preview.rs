use super::EditorSession;

impl EditorSession {
    /// Startpunkt-Marker des Jobs (mm): der gewählte Anker auf der Job-BBox
    /// der aktiven Inhalte. `None` bei Startmodus „Absolut" (der Job liegt,
    /// wo er gezeichnet ist) oder leerem Job. Die BBox entsteht aus denselben
    /// rotierten Konturpunkten wie der JobPlan — ohne Fill-/Raster-Rechnung,
    /// damit der Canvas sie pro Frame abfragen kann.
    pub fn job_start_marker(
        &self,
        selection_only: bool,
        start_mode: luxifer_core::StartMode,
        anchor: luxifer_core::Anchor,
    ) -> Option<(f64, f64)> {
        if start_mode == luxifer_core::StartMode::Absolut {
            return None;
        }
        let mut bbox: Option<(f64, f64, f64, f64)> = None;
        for (i, shape) in self.state.shapes.iter().enumerate() {
            let enabled = self
                .state
                .layers
                .get(shape.layer_id)
                .map(|l| l.enabled)
                .unwrap_or(false);
            if !enabled || (selection_only && !self.state.selected.contains(&i)) {
                continue;
            }
            let (pts, _) = shape.geo.outline_points();
            let c = shape.geo.bbox().center();
            for (x, y) in pts {
                let (x, y) = if shape.rotation.abs() <= f64::EPSILON {
                    (x, y)
                } else {
                    luxifer_core::geometry::rotate_point(x, y, c.0, c.1, shape.rotation)
                };
                bbox = Some(match bbox {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
        Some(anchor.point(bbox?))
    }

    /// Baut die read-only Laser-Vorschau aus demselben JobPlan wie Export und
    /// Treiber. Keine Session-Mutation, keine zweite Geometrie-Wahrheit.
    pub fn job_preview(&self, selection_only: bool) -> luxifer_core::preview::JobPreview {
        let shapes: Vec<luxifer_core::Shape> = if selection_only {
            self.state
                .selected
                .iter()
                .filter_map(|&i| self.state.shapes.get(i).cloned())
                .collect()
        } else {
            self.state.shapes.clone()
        };
        // Bild-Layer werden mit denselben Asset-Pixeln gerastert wie der echte
        // Job (assets::resolve_luma) — die Vorschau zeigt die verarbeitete
        // Rastertextur, nicht das Design-Original.
        let plan = luxifer_core::JobPlan::from_shapes_with_assets(
            &shapes,
            &self.state.layers,
            crate::assets::resolve_luma,
        );
        luxifer_core::preview::JobPreview::from_plan(&plan)
    }
}
