use luxifer_core::{Align, Geo};

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
    assert!(session.undo());
    assert_eq!(session.shapes.len(), 1);
    assert!(session.redo());
    assert!(session.shapes.is_empty());
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
    assert_eq!(session.selected.len(), 2);
}

#[test]
fn mehrere_drag_updates_erzeugen_genau_einen_undo_schritt() {
    let mut session = session_with_rect();
    let original = session.shapes[0].bbox();
    session.begin_edit();
    session.translate_edit(2.0, 0.0);
    session.translate_edit(3.0, 4.0);
    session.commit_edit();
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
        assert_eq!(
            session.shapes[index].bezier.is_some(),
            path == PointPath::Bezier
        );
    }
}

#[test]
fn auswahloperation_ohne_voraussetzung_mutiert_nicht() {
    let mut session = EditorSession::default();
    let error = session.align(Align::Left).unwrap_err();
    assert_eq!(error.code(), "selection_required");
    assert!(!session.dirty);
    assert!(!session.undo());
}

#[test]
fn ausrichten_erzeugt_nur_den_core_undo_schritt() {
    let mut session = session_with_rect();
    let original = session.shapes[0].bbox();
    session.align(Align::Right).unwrap();
    assert_ne!(session.shapes[0].bbox(), original);
    assert!(session.undo());
    assert_eq!(session.shapes[0].bbox(), original);
    assert!(session.redo());
}

#[test]
fn layer_schalter_ist_dirty_und_undo_faehig() {
    let mut session = session_with_rect();
    let original = session.layers[0].visible;
    session.toggle_layer(0, LayerToggle::Visible).unwrap();
    assert_eq!(session.layers[0].visible, !original);
    assert!(session.undo());
    assert_eq!(session.layers[0].visible, original);
}

#[test]
fn ungueltiger_layerindex_liefert_fehler_ohne_mutation() {
    let mut session = session_with_rect();
    let error = session.toggle_layer(4, LayerToggle::Locked).unwrap_err();
    assert_eq!(error.code(), "layer_not_found");
    assert_eq!(session.layers.len(), 1);
}

#[test]
fn layer_verschieben_behaelt_shape_zuordnung_und_ist_undo_faehig() {
    let mut session = session_with_rect();
    session.clear_selection();
    session.activate_color([0x10, 0xB9, 0x81]);
    session.add_box_shape(BoxShape::Rect, [20.0, 0.0], [30.0, 10.0]);
    let second_color = session.layers[1].color;
    session.move_layer(1, 0).unwrap();
    assert_eq!(session.layers[0].color, second_color);
    assert_eq!(session.shapes[1].layer_id, 0);
    assert!(session.undo());
    assert_eq!(session.shapes[1].layer_id, 1);
}
