//! Nativer Editor-Workflow: Auswahl-Aktionen (Löschen, Undo/Redo, Farbe,
//! Anordnen, Gruppieren, Nesting), Layer-Verwaltung und die Geometrie-Dialoge.
//! Validierung, Undo und Berechnung bleiben in Application/Core.

use studio_application::{AppError, LayerParams, LayerToggle};

use super::App;
use crate::ui::{GeoOpDialogState, GeoOpKind, LayerDialogState};

impl App {
    /// Öffnet den passenden Editor für einen doppelt angeklickten Shape:
    /// Bildparameter bei einem Bild-Objekt, Text-Editor bei einem Textblock.
    pub(super) fn edit_shape(&mut self, index: usize) {
        use studio_core::Geo;
        let shapes = &self.session.state().shapes;
        let Some(hit) = shapes.get(index) else {
            return;
        };
        if matches!(hit.geo, Geo::Image { .. }) {
            self.open_image_dialog(index);
            return;
        }
        // Textblock: die Meta liegt am ersten Shape der Gruppe. Anker suchen.
        let anchor = if hit.text_meta.is_some() {
            Some(index)
        } else {
            hit.group_id.and_then(|g| {
                shapes
                    .iter()
                    .position(|s| s.group_id == Some(g) && s.text_meta.is_some())
            })
        };
        if let Some(a) = anchor {
            self.open_text_editor(a);
        }
    }

    pub fn delete_selected(&mut self) {
        if let Err(error) = self.session.delete_selected() {
            self.app_error = Some(error);
        }
    }

    pub fn undo(&mut self) {
        self.session.undo();
    }

    pub fn redo(&mut self) {
        self.session.redo();
    }

    pub fn pick_color(&mut self, c: [u8; 3]) {
        self.session.activate_color(c);
        self.refresh_accent();
    }

    // ---- Sofort-Aktionen auf der Auswahl (Werkzeugleiste + Arrange) ----------

    pub fn mirror_h(&mut self) {
        let result = self.session.mirror(studio_core::Axis::Vertical);
        self.report(result);
    }
    pub fn mirror_v(&mut self) {
        let result = self.session.mirror(studio_core::Axis::Horizontal);
        self.report(result);
    }
    pub fn insert_coasters(&mut self, round: bool) {
        self.session.insert_coasters(round);
        self.fit_all();
    }
    pub fn align(&mut self, kind: studio_core::Align) {
        let result = self.session.align(kind);
        self.report(result);
    }
    pub fn distribute(&mut self, kind: studio_core::Distribute) {
        let result = self.session.distribute(kind);
        self.report(result);
    }
    pub fn group(&mut self) {
        let result = self.session.group();
        self.report(result);
    }
    pub fn ungroup(&mut self) {
        let result = self.session.ungroup();
        self.report(result);
    }
    pub fn resize_selection(&mut self, width: f64, height: f64) {
        let result = self.session.resize_selection(width, height);
        self.report(result);
    }
    pub fn nest(&mut self, gap: f64) {
        let result = self.session.nest(gap);
        self.report(result);
    }
    pub fn nest_fill(&mut self, gap: f64) {
        let result = self.session.nest_fill(gap);
        self.report(result);
    }
    pub fn selection_count(&self) -> usize {
        self.session.selected.len()
    }

    pub fn toggle_layer(&mut self, index: usize, toggle: LayerToggle) {
        let result = self.session.toggle_layer(index, toggle);
        self.report(result);
    }

    pub fn move_layer(&mut self, from: usize, to: usize) {
        let result = self.session.move_layer(from, to);
        self.report(result);
    }

    /// Öffnet den Layer-Parameter-Dialog mit den aktuellen Werten als Entwurf.
    pub fn open_layer_dialog(&mut self, index: usize) {
        if let Some(layer) = self.session.layers.get(index) {
            self.layer_dialog = Some(LayerDialogState {
                index,
                params: LayerParams::from_layer(layer),
            });
        }
    }

    /// Übernimmt den Dialogentwurf über die Session. Bei Erfolg true (Dialog
    /// schließen); bei Validierungsfehler bleibt der Dialog offen und der Fehler
    /// erscheint im zentralen Kanal.
    pub fn commit_layer_dialog(&mut self) -> bool {
        let Some(st) = self.layer_dialog.as_ref() else {
            return false;
        };
        let index = st.index;
        let params = st.params.clone();
        match self.session.set_layer_params(index, params) {
            Ok(()) => true,
            Err(error) => {
                self.app_error = Some(error);
                false
            }
        }
    }

    /// Sofort-Aktion aus der Werkzeugleiste. Fillet und Offset sind interaktive
    /// Canvas-Werkzeuge; Boolean und Muster öffnen Parameterdialoge.
    pub fn begin_action(&mut self, a: crate::tools::ToolAction) {
        use crate::tools::ToolAction as A;
        match a {
            A::Boolean => self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::Boolean)),
            A::Fillet => {
                self.geo_op_dialog = None;
                self.canvas.bridge = None;
                self.canvas.offset = None;
                self.canvas.tool = crate::tools::Tool::Fillet;
                let draft = crate::canvas::state::FilletDraft {
                    shape_index: self.session.selected.first().copied(),
                    ..Default::default()
                };
                self.canvas.fillet = Some(draft);
            }
            A::Offset => {
                self.geo_op_dialog = None;
                self.canvas.bridge = None;
                self.canvas.tool = crate::tools::Tool::Offset;
                self.canvas.offset = Some(Default::default());
            }
            A::PatternFill => {
                self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::PatternFill))
            }
        }
    }

    /// Bestätigt den schwebenden Haltesteg-Entwurf: trennt die gekreuzten
    /// Konturen im Steg-Band auf (Core) und schließt die Schnittkanten quer.
    /// Bei Fehlschlag (Linie kreuzt nichts) bleibt der Entwurf zum Nachfassen
    /// stehen.
    pub fn commit_bridge(&mut self) {
        let Some(d) = self.canvas.bridge else {
            return;
        };
        self.canvas.bridge_width = d.width;
        match self
            .session
            .bridge((d.p0[0], d.p0[1]), (d.p1[0], d.p1[1]), d.width)
        {
            Ok(()) => {
                self.canvas.bridge = None;
                self.refresh_accent();
                self.toasts.success("Haltesteg eingefügt");
            }
            Err(error) => self.toasts.error(error.message().to_string()),
        }
    }

    /// Verwirft den schwebenden Haltesteg-Entwurf.
    pub fn cancel_bridge(&mut self) {
        self.canvas.bridge = None;
    }

    /// Führt die im Geometrie-Dialog parametrierte Operation über die Session
    /// aus. Erfolg → Dialog schließen; Auswahl-/Voraussetzungsfehler → offen +
    /// Fehlerkanal.
    pub fn commit_geo_op(&mut self) -> bool {
        let Some(st) = self.geo_op_dialog.as_ref() else {
            return false;
        };
        let result = match st.kind {
            GeoOpKind::Boolean => self.session.boolean(st.bool_op),
            GeoOpKind::PatternFill => self.session.pattern_fill(&st.fill),
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.app_error = Some(error);
                false
            }
        }
    }

    pub fn refresh_offset_preview(&mut self) {
        let Some(draft) = self.canvas.offset.as_mut() else {
            return;
        };
        let result = draft.distance().ok_or_else(|| {
            studio_application::AppError::new(
                "offset_number",
                "Bitte einen gültigen Abstand in Millimetern eingeben.",
            )
        });
        match result.and_then(|distance| self.session.offset_preview(distance)) {
            Ok(preview) => {
                draft.preview = preview;
                draft.error = None;
            }
            Err(error) => {
                draft.preview.clear();
                draft.error = Some(error.message().to_owned());
            }
        }
    }

    pub fn commit_offset(&mut self) {
        let Some(distance) = self
            .canvas
            .offset
            .as_ref()
            .and_then(|draft| draft.distance())
        else {
            return;
        };
        match self.session.offset(distance) {
            Ok(()) => {
                self.canvas.offset = None;
                self.canvas.tool = crate::tools::Tool::Select;
                self.refresh_accent();
                self.toasts.success("Offset erstellt");
            }
            Err(error) => {
                if let Some(draft) = self.canvas.offset.as_mut() {
                    draft.error = Some(error.message().to_owned());
                }
            }
        }
    }

    pub fn cancel_offset(&mut self) {
        self.canvas.offset = None;
        self.canvas.tool = crate::tools::Tool::Select;
    }

    pub fn commit_fillet(&mut self) {
        let Some(draft) = self.canvas.fillet.as_ref() else {
            return;
        };
        let Some(shape_index) = draft.shape_index else {
            return;
        };
        match self.session.fillet_corners(shape_index, &draft.radii) {
            Ok(count) => {
                self.canvas.fillet = None;
                self.canvas.tool = crate::tools::Tool::Select;
                self.refresh_accent();
                self.toasts.success(format!("{count} Fillets übernommen"));
            }
            Err(error) => {
                if let Some(draft) = self.canvas.fillet.as_mut() {
                    draft.error = Some(error.message().to_owned());
                }
            }
        }
    }

    pub fn cancel_fillet(&mut self) {
        self.canvas.fillet = None;
        self.canvas.tool = crate::tools::Tool::Select;
    }

    fn report(&mut self, result: Result<(), AppError>) {
        if let Err(error) = result {
            self.app_error = Some(error);
        }
    }
}
