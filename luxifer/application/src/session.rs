use std::ops::{Deref, DerefMut};

use luxifer_core::AppState;

use crate::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxShape {
    Rect,
    Ellipse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointPath {
    Polyline,
    Spline,
    Bezier,
}

/// Laufende, UI-unabhängige Editor-Sitzung.
///
/// `Deref`/`DerefMut` sind eine bewusst vorübergehende Migrationsbrücke für
/// noch nicht extrahierte Native-Abläufe. Neue Anwendungsfälle erhalten
/// benannte Methoden auf `EditorSession`; der Direktzugriff wird mit jedem
/// vertikalen Schnitt kleiner und am Ende entfernt.
#[derive(Debug, Default)]
pub struct EditorSession {
    state: AppState,
    edit_start: Option<AppState>,
}

impl EditorSession {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            edit_start: None,
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut_for_migration(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn replace_state(&mut self, state: AppState) -> AppState {
        self.edit_start = None;
        std::mem::replace(&mut self.state, state)
    }

    pub fn select_at(&mut self, x: f64, y: f64, tolerance: f64, additive: bool) -> Option<usize> {
        let hit = self.state.hit_test(x, y, tolerance);
        match hit {
            Some(index) if additive => {
                if let Some(position) = self.state.selected.iter().position(|&item| item == index) {
                    self.state.selected.remove(position);
                } else {
                    self.state.selected.push(index);
                }
            }
            Some(index) if !self.state.selected.contains(&index) => {
                self.state.selected = vec![index];
            }
            None if !additive => self.state.selected.clear(),
            _ => {}
        }
        self.state.expand_selection_to_groups();
        hit
    }

    pub fn select_rect(&mut self, start: [f64; 2], end: [f64; 2]) {
        self.state
            .select_in_rect(start[0], start[1], end[0], end[1]);
        self.state.expand_selection_to_groups();
    }

    pub fn clear_selection(&mut self) {
        self.state.selected.clear();
    }

    /// Beginnt eine zusammenhängende direkte Manipulation. Beliebig viele
    /// Zwischenstände bilden danach genau einen Undo-Schritt.
    pub fn begin_edit(&mut self) {
        if self.edit_start.is_none() {
            self.edit_start = Some(self.state.clone());
            self.state.push_undo();
        }
    }

    pub fn edit_active(&self) -> bool {
        self.edit_start.is_some()
    }

    pub fn translate_edit(&mut self, dx: f64, dy: f64) {
        debug_assert!(self.edit_active(), "translate_edit ohne begin_edit");
        self.state.translate_selected(dx, dy);
    }

    pub fn scale_edit(&mut self, start: luxifer_core::BBox, target: luxifer_core::BBox) {
        debug_assert!(self.edit_active(), "scale_edit ohne begin_edit");
        self.state.scale_selection_to(start, target);
    }

    pub fn rotate_edit(&mut self, degrees: f64) {
        debug_assert!(self.edit_active(), "rotate_edit ohne begin_edit");
        self.state.rotate_selection(degrees);
    }

    pub fn commit_edit(&mut self) {
        if self.edit_start.take().is_some() {
            self.state.discard_last_undo_if_no_change();
        }
    }

    pub fn cancel_edit(&mut self) -> bool {
        let Some(start) = self.edit_start.take() else {
            return false;
        };
        self.state = start;
        true
    }

    pub fn add_box_shape(
        &mut self,
        shape: BoxShape,
        start: [f64; 2],
        end: [f64; 2],
    ) -> Option<usize> {
        let x = start[0].min(end[0]);
        let y = start[1].min(end[1]);
        let w = (start[0] - end[0]).abs();
        let h = (start[1] - end[1]).abs();
        if w < 0.5 || h < 0.5 {
            return None;
        }
        let geometry = match shape {
            BoxShape::Rect => luxifer_core::Geo::Rect { x, y, w, h },
            BoxShape::Ellipse => luxifer_core::Geo::Ellipse {
                cx: x + w / 2.0,
                cy: y + h / 2.0,
                rx: w / 2.0,
                ry: h / 2.0,
            },
        };
        Some(self.state.add_shape(geometry))
    }

    pub fn add_line(&mut self, start: [f64; 2], end: [f64; 2]) -> Option<usize> {
        if (start[0] - end[0]).hypot(start[1] - end[1]) < 0.5 {
            return None;
        }
        Some(self.state.add_shape(luxifer_core::Geo::Polyline {
            pts: vec![(start[0], start[1]), (end[0], end[1])],
            closed: false,
        }))
    }

    pub fn add_polygon(
        &mut self,
        shape: luxifer_core::PolyShape,
        center: [f64; 2],
        edge: [f64; 2],
    ) -> Option<usize> {
        let radius = (center[0] - edge[0]).hypot(center[1] - edge[1]);
        if radius < 1.0 {
            return None;
        }
        let pts = shape.points(center[0], center[1], radius, 0.0);
        Some(
            self.state
                .add_shape(luxifer_core::Geo::Polyline { pts, closed: true }),
        )
    }

    pub fn add_point_path(&mut self, path: PointPath, points: Vec<(f64, f64)>) -> Option<usize> {
        if points.len() < 2 {
            return None;
        }
        let index = match path {
            PointPath::Polyline => self.state.add_shape(luxifer_core::Geo::Polyline {
                pts: points,
                closed: false,
            }),
            PointPath::Spline => {
                let pts = luxifer_core::geometry::catmull_rom(&points, false, 12);
                self.state
                    .add_shape(luxifer_core::Geo::Polyline { pts, closed: false })
            }
            PointPath::Bezier => self.state.add_bezier(points, false),
        };
        Some(index)
    }

    pub fn delete_selected(&mut self) -> Result<(), AppError> {
        if self.state.selected.is_empty() {
            return Err(AppError::new(
                "selection_required",
                "Zum Löschen muss mindestens ein Objekt ausgewählt sein.",
            ));
        }
        self.state.delete_selected();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        self.state.undo()
    }

    pub fn redo(&mut self) -> bool {
        self.state.redo()
    }
}

impl Deref for EditorSession {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for EditorSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

#[cfg(test)]
mod tests {
    use luxifer_core::Geo;

    use super::*;

    fn session_with_rect() -> EditorSession {
        let mut state = AppState::new();
        state.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        EditorSession::new(state)
    }

    #[test]
    fn loeschen_ohne_auswahl_liefert_stabilen_fehler_ohne_mutation() {
        let mut session = EditorSession::default();

        let error = session.delete_selected().unwrap_err();

        assert_eq!(error.code(), "selection_required");
        assert!(session.shapes.is_empty());
        assert!(!session.dirty);
    }

    #[test]
    fn loeschen_undo_und_redo_bleiben_ein_zusammenhaengender_ablauf() {
        let mut session = session_with_rect();

        session.delete_selected().unwrap();
        assert!(session.shapes.is_empty());
        assert!(session.dirty);

        assert!(session.undo());
        assert_eq!(session.shapes.len(), 1);
        assert_eq!(session.selected, vec![0]);

        assert!(session.redo());
        assert!(session.shapes.is_empty());
        assert!(session.selected.is_empty());
    }

    #[test]
    fn undo_und_redo_ohne_historie_sind_sichere_no_ops() {
        let mut session = EditorSession::default();

        assert!(!session.undo());
        assert!(!session.redo());
        assert!(!session.dirty);
    }

    #[test]
    fn additive_auswahl_toggelt_und_erweitert_gruppen() {
        let mut state = AppState::new();
        state.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        state.add_shape(Geo::Rect {
            x: 20.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        state.shapes[0].group_id = Some(1);
        state.shapes[1].group_id = Some(1);
        state.selected.clear();
        let mut session = EditorSession::new(state);

        assert_eq!(session.select_at(5.0, 5.0, 0.0, false), Some(0));
        assert_eq!(session.selected, vec![0, 1]);

        session.select_at(5.0, 5.0, 0.0, true);
        // Gruppen bleiben eine unteilbare Auswahl.
        assert_eq!(session.selected.len(), 2);
        assert!(session.selected.contains(&0));
        assert!(session.selected.contains(&1));
    }

    #[test]
    fn mehrere_drag_updates_erzeugen_genau_einen_undo_schritt() {
        let mut session = session_with_rect();
        let original = session.shapes[0].bbox();

        session.begin_edit();
        session.translate_edit(2.0, 0.0);
        session.translate_edit(3.0, 4.0);
        session.commit_edit();
        assert_eq!(session.shapes[0].bbox().x, original.x + 5.0);
        assert_eq!(session.shapes[0].bbox().y, original.y + 4.0);

        assert!(session.undo());
        assert_eq!(session.shapes[0].bbox(), original);
        assert!(session.redo());
        assert_eq!(session.shapes[0].bbox().x, original.x + 5.0);
    }

    #[test]
    fn abgebrochene_geste_stellt_zustand_und_historie_wieder_her() {
        let mut session = session_with_rect();
        let original = session.shapes[0].bbox();

        session.begin_edit();
        session.translate_edit(50.0, 20.0);
        assert!(session.cancel_edit());

        assert_eq!(session.shapes[0].bbox(), original);
        assert!(!session.edit_active());
        // Der abgebrochene Edit hat keinen Undo-Eintrag hinterlassen. Der noch
        // vorhandene Eintrag stammt ausschließlich vom initialen add_shape.
        assert!(session.undo());
        assert!(session.shapes.is_empty());
    }

    #[test]
    fn box_und_linie_verwerfen_zu_kleine_gesten_ohne_undo_leiche() {
        let mut session = EditorSession::default();

        assert_eq!(
            session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [0.2, 0.2]),
            None
        );
        assert_eq!(session.add_line([0.0, 0.0], [0.1, 0.1]), None);
        assert!(session.shapes.is_empty());
        assert!(!session.undo());
    }

    #[test]
    fn gezeichnete_form_ist_selektiert_und_einzeln_undo_faehig() {
        let mut session = EditorSession::default();

        let index = session
            .add_box_shape(BoxShape::Ellipse, [20.0, 30.0], [0.0, 10.0])
            .unwrap();

        assert_eq!(session.selected, vec![index]);
        assert_eq!(session.shapes.len(), 1);
        assert!(session.undo());
        assert!(session.shapes.is_empty());
        assert!(session.redo());
        assert_eq!(session.shapes.len(), 1);
    }

    #[test]
    fn punktpfade_werden_nach_typ_im_core_erzeugt() {
        let points = vec![(0.0, 0.0), (10.0, 20.0), (20.0, 0.0)];

        for path in [PointPath::Polyline, PointPath::Spline, PointPath::Bezier] {
            let mut session = EditorSession::default();
            let index = session.add_point_path(path, points.clone()).unwrap();
            assert_eq!(session.selected, vec![index]);
            assert_eq!(session.shapes.len(), 1);
            assert_eq!(
                session.shapes[index].bezier.is_some(),
                path == PointPath::Bezier
            );
        }
    }
}
