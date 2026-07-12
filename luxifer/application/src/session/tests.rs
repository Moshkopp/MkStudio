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
fn is_dirty_und_mark_saved_bilden_den_dirty_guard_ab() {
    // Frischer Zustand ist sauber; eine Mutation macht ihn dirty; mark_saved
    // (nach erfolgreichem Speichern) setzt ihn zurück.
    let mut session = EditorSession::default();
    assert!(!session.is_dirty());
    session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [10.0, 10.0]);
    assert!(session.is_dirty());
    session.mark_saved();
    assert!(!session.is_dirty());
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
    assert_ne!(session.shapes[0].bbox(), original);
}

#[test]
fn layer_schalter_ist_dirty_und_undo_faehig() {
    let mut session = session_with_rect();
    let original = session.layers[0].visible;
    session.toggle_layer(0, LayerToggle::Visible).unwrap();
    assert_eq!(session.layers[0].visible, !original);
    assert!(session.dirty);
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

fn valid_params() -> LayerParams {
    LayerParams {
        name: "Kontur".into(),
        mode: luxifer_core::LayerMode::Cut,
        speed_mm_s: 120.0,
        power_pct: 60.0,
        min_power_pct: 20.0,
        passes: 2,
        air_assist: true,
        line_step_mm: 0.2,
        dpi: 300.0,
        bidirectional: false,
    }
}

#[test]
fn layer_parameter_vollstaendig_setzen_ist_ein_undo_schritt() {
    let mut session = session_with_rect();
    let before = session.layers[0].clone();
    session.set_layer_params(0, valid_params()).unwrap();
    let l = &session.layers[0];
    assert_eq!(l.name, "Kontur");
    assert_eq!(l.speed_mm_s, 120.0);
    assert_eq!(l.power_pct, 60.0);
    assert_eq!(l.min_power_pct, 20.0);
    assert_eq!(l.passes, 2);
    assert!(l.air_assist);
    assert!(!l.bidirectional);
    assert!(session.dirty);
    assert!(session.undo());
    assert_eq!(session.layers[0], before);
    assert!(session.redo());
    assert_eq!(session.layers[0].name, "Kontur");
}

#[test]
fn ungueltige_leistung_ausserhalb_prozentbereich_mutiert_nicht() {
    let mut session = session_with_rect();
    session.state_mut_for_migration().dirty = false;
    let before = session.layers[0].clone();
    let mut params = valid_params();
    params.power_pct = 140.0;
    let error = session.set_layer_params(0, params).unwrap_err();
    assert_eq!(error.code(), "power_range");
    assert_eq!(session.layers[0], before);
    assert!(!session.dirty);
    // Kein zusätzlicher Undo-Punkt: der einzige Undo (aus dem Setup) entfernt
    // das Rechteck, danach ist die Historie leer.
    assert!(session.undo());
    assert!(session.shapes.is_empty());
    assert!(!session.undo());
}

#[test]
fn minimale_leistung_ueber_maximaler_wird_abgewiesen() {
    let mut session = session_with_rect();
    session.state_mut_for_migration().dirty = false;
    let mut params = valid_params();
    params.min_power_pct = 80.0;
    params.power_pct = 50.0;
    let error = session.set_layer_params(0, params).unwrap_err();
    assert_eq!(error.code(), "power_order");
    assert!(!session.dirty);
}

#[test]
fn nicht_positive_geschwindigkeit_wird_abgewiesen() {
    let mut session = session_with_rect();
    session.state_mut_for_migration().dirty = false;
    let mut params = valid_params();
    params.speed_mm_s = 0.0;
    let error = session.set_layer_params(0, params).unwrap_err();
    assert_eq!(error.code(), "speed_invalid");
    assert!(!session.dirty);
    // Nur der Setup-Undo-Punkt existiert; die Validierung fügte keinen hinzu.
    assert!(session.undo());
    assert!(session.shapes.is_empty());
    assert!(!session.undo());
}

#[test]
fn parameter_setzen_bei_ungueltigem_index_liefert_fehler() {
    let mut session = session_with_rect();
    session.state_mut_for_migration().dirty = false;
    let error = session.set_layer_params(9, valid_params()).unwrap_err();
    assert_eq!(error.code(), "layer_not_found");
    assert!(!session.dirty);
}

#[test]
fn image_layer_kann_nicht_zu_vektor_umgewandelt_werden() {
    let mut session = session_with_rect();
    session.layers[0].mode = luxifer_core::LayerMode::Image;
    session.state_mut_for_migration().dirty = false;
    let before = session.layers[0].clone();
    let mut params = valid_params();
    params.mode = luxifer_core::LayerMode::Cut;
    let error = session.set_layer_params(0, params).unwrap_err();
    assert_eq!(error.code(), "image_layer_mode");
    assert_eq!(session.layers[0], before);
    assert!(!session.dirty);
}

#[test]
fn vektor_layer_kann_nicht_versehentlich_image_werden() {
    let mut session = session_with_rect();
    session.state_mut_for_migration().dirty = false;
    let mut params = valid_params();
    params.mode = luxifer_core::LayerMode::Image;
    let error = session.set_layer_params(0, params).unwrap_err();
    assert_eq!(error.code(), "image_layer_mode");
    assert!(!session.dirty);
}

#[test]
fn image_layer_darf_seine_bildparameter_aendern() {
    let mut session = session_with_rect();
    session.layers[0].mode = luxifer_core::LayerMode::Image;
    let mut params = LayerParams::from_layer(&session.layers[0]);
    params.dpi = 508.0;
    params.bidirectional = false;
    session.set_layer_params(0, params).unwrap();
    assert_eq!(session.layers[0].dpi, 508.0);
    assert!(!session.layers[0].bidirectional);
    assert_eq!(session.layers[0].mode, luxifer_core::LayerMode::Image);
}

#[test]
fn cut_layer_mit_altem_null_wert_bleibt_speicherbar() {
    // Regression: DPI/Zeilenabstand werden nur im relevanten Modus geprüft.
    // Ein Cut-Layer mit einem unsichtbaren `dpi = 0` aus einem Altprojekt darf
    // sich weiter speichern lassen.
    let mut session = session_with_rect();
    let mut params = valid_params();
    params.mode = luxifer_core::LayerMode::Cut;
    params.dpi = 0.0;
    params.line_step_mm = 0.0;
    session.set_layer_params(0, params).unwrap();
    assert_eq!(session.layers[0].mode, luxifer_core::LayerMode::Cut);
}

#[test]
fn fill_layer_verlangt_positiven_zeilenabstand() {
    let mut session = session_with_rect();
    session.state_mut_for_migration().dirty = false;
    let mut params = valid_params();
    params.mode = luxifer_core::LayerMode::Fill;
    params.line_step_mm = 0.0;
    let error = session.set_layer_params(0, params).unwrap_err();
    assert_eq!(error.code(), "line_step_invalid");
    assert!(!session.dirty);
}

#[test]
fn raster_layer_verlangt_positive_dpi() {
    let mut session = session_with_rect();
    session.state_mut_for_migration().dirty = false;
    let mut params = valid_params();
    params.mode = luxifer_core::LayerMode::Raster;
    params.dpi = 0.0;
    let error = session.set_layer_params(0, params).unwrap_err();
    assert_eq!(error.code(), "dpi_invalid");
    assert!(!session.dirty);
}

#[test]
fn nan_werte_gelten_als_ungueltig_und_mutieren_nicht() {
    // Zugesicherte NaN-Behandlung: eine reine `<= 0.0`-Prüfung ließe NaN durch.
    for (mut params, code) in [
        {
            let mut p = valid_params();
            p.speed_mm_s = f64::NAN;
            (p, "speed_invalid")
        },
        {
            let mut p = valid_params();
            p.power_pct = f64::NAN;
            (p, "power_range")
        },
        {
            let mut p = valid_params();
            p.mode = luxifer_core::LayerMode::Fill;
            p.line_step_mm = f64::NAN;
            (p, "line_step_invalid")
        },
        {
            let mut p = valid_params();
            p.mode = luxifer_core::LayerMode::Raster;
            p.dpi = f64::NAN;
            (p, "dpi_invalid")
        },
    ] {
        params.name = "NaN".into();
        let mut session = session_with_rect();
        session.state_mut_for_migration().dirty = false;
        let error = session.set_layer_params(0, params).unwrap_err();
        assert_eq!(error.code(), code);
        assert!(!session.dirty);
    }
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
    assert_eq!(session.layers[1].color, second_color);
    assert_eq!(session.shapes[1].layer_id, 1);
}
