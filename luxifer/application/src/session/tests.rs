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
fn select_all_waehlt_jedes_objekt_genau_einmal() {
    let mut state = AppState::new();
    for x in [0.0, 20.0, 40.0] {
        state.add_shape(Geo::Rect {
            x,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
    }
    state.selected.clear();
    let mut session = EditorSession::new(state);
    session.select_all();
    assert_eq!(session.selected, vec![0, 1, 2]);
    // Idempotent — kein Toggle wie bei der additiven Auswahl.
    session.select_all();
    assert_eq!(session.selected, vec![0, 1, 2]);
}

#[test]
fn numerische_auswahlgroesse_skaliert_und_ist_ein_undo_schritt() {
    let mut session = session_with_rect();
    session.resize_selection(25.0, 15.0).unwrap();
    let resized = session.selection_bbox().unwrap();
    assert!((resized.w - 25.0).abs() < 1e-9);
    assert!((resized.h - 15.0).abs() < 1e-9);

    assert!(session.undo());
    let original = session.selection_bbox().unwrap();
    assert!((original.w - 10.0).abs() < 1e-9);
    assert!((original.h - 10.0).abs() < 1e-9);
}

#[test]
fn numerische_mehrfachauswahl_skaliert_gemeinsame_box() {
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
    state.selected = vec![0, 1];
    let mut session = EditorSession::new(state);

    session.resize_selection(60.0, 20.0).unwrap();
    let bbox = session.selection_bbox().unwrap();
    assert!((bbox.w - 60.0).abs() < 1e-9);
    assert!((bbox.h - 20.0).abs() < 1e-9);
    assert!((session.shapes[1].bbox().x - 40.0).abs() < 1e-9);
}

#[test]
fn numerische_auswahlgroesse_weist_ungueltige_werte_ohne_mutation_ab() {
    let mut session = session_with_rect();
    let before = session.selection_bbox().unwrap();
    let before_dirty = session.dirty;
    let error = session.resize_selection(0.0, f64::NAN).unwrap_err();
    assert_eq!(error.code(), "invalid_selection_size");
    assert_eq!(session.selection_bbox(), Some(before));
    assert_eq!(session.dirty, before_dirty);
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
        let index = session.add_point_path(path, points.clone(), false).unwrap();
        assert_eq!(session.selected, vec![index]);
        assert_eq!(session.shapes.len(), 1);
        assert_eq!(
            session.shapes[index].bezier.is_some(),
            path == PointPath::Bezier
        );
    }
}

#[test]
fn fertige_bezier_knoten_behalten_ihre_tangenten() {
    let nodes = vec![
        luxifer_core::bezier::BezierNode {
            p: (0.0, 0.0),
            h_in: Some((-5.0, 0.0)),
            h_out: Some((5.0, 0.0)),
        },
        luxifer_core::bezier::BezierNode::corner((20.0, 10.0)),
    ];
    let mut session = EditorSession::default();
    let index = session.add_bezier_nodes(nodes.clone(), true).unwrap();
    assert_eq!(session.shapes[index].bezier.as_ref().unwrap().nodes, nodes);
    assert!(session.shapes[index].bezier.as_ref().unwrap().closed);
    assert!(session.undo());
    assert!(session.shapes.is_empty());
}

#[test]
fn textblock_ersetzen_erzeugt_genau_einen_undo_schritt() {
    let mut session = EditorSession::default();
    let old = vec![(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], true)];
    let meta = luxifer_core::TextMeta {
        text: "Alt".into(),
        font_path: "font.ttf".into(),
        font_asset: None,
        size_mm: 10.0,
    };
    let indices = session.add_text_block(old, meta);
    session.mark_saved();
    let new = vec![(vec![(0.0, 0.0), (20.0, 0.0), (20.0, 10.0)], true)];
    let meta = luxifer_core::TextMeta {
        text: "Neu".into(),
        font_path: "font.ttf".into(),
        font_asset: None,
        size_mm: 10.0,
    };

    session.replace_text_block(indices[0], new, meta).unwrap();
    assert_eq!(session.shapes[0].text_meta.as_ref().unwrap().text, "Neu");
    assert!(session.undo());
    assert_eq!(session.shapes[0].text_meta.as_ref().unwrap().text, "Alt");
    // Der nächste Undo gehört bereits zum ursprünglichen Einfügen; ein
    // doppelter Replace-Snapshot würde hier den alten Text nochmals behalten.
    assert!(session.undo());
    assert!(session.shapes.is_empty());
}

#[test]
fn job_preview_ist_abgeleitete_read_only_sicht() {
    let mut session = EditorSession::default();
    session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [20.0, 10.0]);
    let dirty_before = session.is_dirty();

    let preview = session.job_preview(false);

    assert_eq!(preview.moves.len(), 4);
    assert!((preview.total_len_mm - 60.0).abs() < 1e-6);
    assert_eq!(session.is_dirty(), dirty_before);
}

#[test]
fn job_preview_rastert_bilder_nicht_unsichtbar_im_ui_thread() {
    let mut state = AppState::new();
    state.add_image("grosses-asset".into(), 0.0, 0.0, 500.0, 300.0);
    let session = EditorSession::new(state);

    let preview = session.job_preview(false);

    assert!(preview.is_empty());
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

fn session_with_image() -> EditorSession {
    let mut state = AppState::new();
    state.add_image("asset-1".into(), 0.0, 0.0, 40.0, 30.0);
    state.selected.clear();
    EditorSession::new(state)
}

#[test]
fn bildparameter_setzen_ist_ein_undo_schritt() {
    use luxifer_core::{ImageMode, ImageParams};
    let mut session = session_with_image();
    session.state_mut_for_migration().dirty = false;
    let params = ImageParams {
        mode: ImageMode::Floyd,
        brightness: 20,
        gamma: 1.5,
        ..Default::default()
    };

    session.set_image_params(0, params).unwrap();
    assert!(session.dirty);
    if let luxifer_core::Geo::Image { params: p, .. } = &session.shapes[0].geo {
        assert_eq!(p.mode, ImageMode::Floyd);
        assert_eq!(p.brightness, 20);
    } else {
        panic!("erwartet Geo::Image");
    }
    // Genau ein Undo-Schritt: danach ist die Historie leer (kein Setup-Undo).
    assert!(session.undo());
    if let luxifer_core::Geo::Image { params: p, .. } = &session.shapes[0].geo {
        assert_eq!(p.mode, ImageMode::Grayscale);
    }
}

#[test]
fn bildparameter_auf_nicht_bild_liefert_fehler() {
    use luxifer_core::ImageParams;
    let mut session = session_with_rect(); // Rechteck, kein Bild
    session.state_mut_for_migration().dirty = false;
    let error = session
        .set_image_params(0, ImageParams::default())
        .unwrap_err();
    assert_eq!(error.code(), "not_an_image");
    assert!(!session.dirty);
}

#[test]
fn ungueltiges_gamma_wird_abgewiesen() {
    use luxifer_core::ImageParams;
    let mut session = session_with_image();
    session.state_mut_for_migration().dirty = false;
    let params = ImageParams {
        gamma: 5.0, // außerhalb 0.1..3.0
        ..Default::default()
    };
    let error = session.set_image_params(0, params).unwrap_err();
    assert_eq!(error.code(), "image_gamma");
    assert!(!session.dirty);
}

#[test]
fn ungueltige_helligkeit_wird_abgewiesen() {
    use luxifer_core::ImageParams;
    let mut session = session_with_image();
    session.state_mut_for_migration().dirty = false;
    let params = ImageParams {
        brightness: 200,
        ..Default::default()
    };
    let error = session.set_image_params(0, params).unwrap_err();
    assert_eq!(error.code(), "image_brightness");
    assert!(!session.dirty);
}

#[test]
fn job_preview_rastert_bild_assets() {
    // Prozessglobales Datenverzeichnis → gemeinsamer Test-Lock.
    let _g = crate::test_env::with_temp_dir("preview_raster");
    let png = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../native/tests/fixtures/test2x2.png"
    ));
    // Fixture optional: wenn nicht vorhanden, Test überspringen (CI-tolerant).
    let Ok(bytes) = png else {
        eprintln!("Fixture fehlt — Test übersprungen");
        return;
    };
    let meta = luxifer_core::import_image(&luxifer_core::assets_dir(), &bytes, "test.png")
        .expect("import_image");

    let mut state = AppState::new();
    state.add_image(meta.id.clone(), 0.0, 0.0, 10.0, 10.0);
    let session = EditorSession::new(state);
    let preview = session.job_preview(false);

    // Der Bild-Layer liefert die verarbeitete Rastertextur an der mm-Position —
    // keine Moves (Bilder sind Texturen, ADR 0008 §2).
    assert_eq!(preview.rasters.len(), 1);
    let tex = &preview.rasters[0];
    assert!(tex.width > 0 && tex.height > 0);
    assert_eq!((tex.x, tex.y, tex.w, tex.h), (0.0, 0.0, 10.0, 10.0));
}

#[test]
fn job_preview_ueberspringt_fehlende_assets_ohne_panik() {
    let _g = crate::test_env::with_temp_dir("preview_raster_missing");
    let mut state = AppState::new();
    state.add_image("gibt-es-nicht".into(), 0.0, 0.0, 10.0, 10.0);
    let session = EditorSession::new(state);
    let preview = session.job_preview(false);
    assert!(preview.rasters.is_empty());
    assert!(preview.moves.is_empty());
}

#[test]
fn pattern_fill_fuellt_geschlossene_kontur_auf_eigenem_layer() {
    let mut session = session_with_rect();
    session.selected = vec![0];
    let layers_before = session.state().layers.len();
    let shapes_before = session.state().shapes.len();

    session
        .pattern_fill(&luxifer_core::pattern_fill::FillParams::default())
        .expect("Füllung");

    // Muster-Konturen entstanden, auf einem eigenen Layer (Farbe = Layer).
    assert!(session.state().shapes.len() > shapes_before);
    assert_eq!(session.state().layers.len(), layers_before + 1);

    // Genau ein Undo-Schritt: rückgängig stellt den Ausgangszustand her.
    assert!(session.undo());
    assert_eq!(session.state().shapes.len(), shapes_before);
}

#[test]
fn pattern_fill_weist_fehler_stabil_ab() {
    use luxifer_core::pattern_fill::{FillParams, Pattern};

    // Ohne Auswahl.
    let mut session = session_with_rect();
    session.selected = vec![];
    let err = session.pattern_fill(&FillParams::default()).unwrap_err();
    assert_eq!(err.code(), "selection_required");

    // Ungültiger Abstand.
    session.selected = vec![0];
    let err = session
        .pattern_fill(&FillParams {
            gap_y: 0.0,
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err.code(), "pattern_gap");

    // Ungültige Elementgröße bei Formen-Muster.
    let err = session
        .pattern_fill(&FillParams {
            pattern: Pattern::Circles,
            size: 0.0,
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err.code(), "pattern_size");

    // Offene Kontur (Linie): nichts zu füllen → Fehler statt stiller No-Op.
    let mut session = EditorSession::default();
    let idx = session.add_line([0.0, 0.0], [50.0, 0.0]).expect("Linie");
    session.selected = vec![idx];
    let shapes_before = session.state().shapes.len();
    let err = session.pattern_fill(&FillParams::default()).unwrap_err();
    assert_eq!(err.code(), "pattern_no_closed");
    assert_eq!(session.state().shapes.len(), shapes_before);
}

/// Erzeugt ein 20×20-PNG (weiß mit schwarzem 10×10-Quadrat in der Mitte) und
/// importiert es in den Asset-Store des aktuellen Test-Datenverzeichnisses.
fn import_test_square() -> String {
    let img = image::GrayImage::from_fn(20, 20, |x, y| {
        if (5..15).contains(&x) && (5..15).contains(&y) {
            image::Luma([0u8])
        } else {
            image::Luma([255u8])
        }
    });
    let mut png: Vec<u8> = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("PNG kodieren");
    luxifer_core::import_image(&luxifer_core::assets_dir(), &png, "quadrat.png")
        .expect("import_image")
        .id
}

#[test]
fn trace_image_erzeugt_geschlossene_konturen_in_mm() {
    let _g = crate::test_env::with_temp_dir("trace");
    let asset = import_test_square();

    let mut state = AppState::new();
    // 20 px auf 40 mm → 2 mm je Pixel; Quadrat liegt bei Pixel 5..15.
    let img_idx = state.add_image(asset, 10.0, 10.0, 40.0, 40.0);
    let mut session = EditorSession::new(state);
    let shapes_before = session.state().shapes.len();

    let idxs = session.trace_image(img_idx, 128, false).expect("Trace");
    assert!(!idxs.is_empty());
    assert!(session.state().shapes.len() > shapes_before);

    // Kontur liegt in mm an der Bildbox: Quadrat ≈ (20..40) mm in beiden Achsen.
    let (pts, closed) = session.state().shapes[idxs[0]].geo.outline_points();
    assert!(closed, "Trace-Konturen sind geschlossen");
    for (x, y) in pts {
        assert!((19.0..=41.0).contains(&x) && (19.0..=41.0).contains(&y));
    }

    // Genau ein Undo-Schritt entfernt die Konturen wieder.
    assert!(session.undo());
    assert_eq!(session.state().shapes.len(), shapes_before);
}

#[test]
fn trace_image_meldet_fehler_stabil() {
    let _g = crate::test_env::with_temp_dir("trace_fehler");
    let asset = import_test_square();

    let mut state = AppState::new();
    let img_idx = state.add_image(asset, 0.0, 0.0, 40.0, 40.0);
    let rect_idx = state.add_shape(luxifer_core::Geo::Rect {
        x: 0.0,
        y: 0.0,
        w: 5.0,
        h: 5.0,
    });
    let mut session = EditorSession::new(state);
    let shapes_before = session.state().shapes.len();

    // Kein Bild-Shape.
    let err = session.trace_image(rect_idx, 128, false).unwrap_err();
    assert_eq!(err.code(), "not_an_image");

    // Schwelle findet nichts (alles heller als 0 → kein Vordergrund).
    let err = session.trace_image(img_idx, 0, false).unwrap_err();
    assert_eq!(err.code(), "trace_empty");
    assert_eq!(
        session.state().shapes.len(),
        shapes_before,
        "keine Mutation"
    );

    // Fehlendes Asset.
    let mut state = AppState::new();
    let idx = state.add_image("fehlt".into(), 0.0, 0.0, 10.0, 10.0);
    let mut session = EditorSession::new(state);
    let err = session.trace_image(idx, 128, false).unwrap_err();
    assert_eq!(err.code(), "asset_read");
}

#[test]
fn job_start_marker_liegt_am_anker_der_job_bbox() {
    use luxifer_core::{Anchor, StartMode};
    // Rechteck (0,0)–(10,10): Anker Mitte → (5,5); Anker NW → (0,0).
    let session = session_with_rect();
    let m = session
        .job_start_marker(false, StartMode::AktuellePosition, Anchor::Center)
        .expect("Marker");
    assert_eq!(m, (5.0, 5.0));
    let m = session
        .job_start_marker(false, StartMode::Benutzerursprung, Anchor::NW)
        .expect("Marker");
    assert_eq!(m, (0.0, 0.0));

    // Absolut: kein Marker (Job liegt, wo er gezeichnet ist).
    assert!(session
        .job_start_marker(false, StartMode::Absolut, Anchor::Center)
        .is_none());

    // Deaktivierter Layer zählt nicht → leerer Job, kein Marker.
    let mut session = session_with_rect();
    session.state_mut_for_migration().layers[0].enabled = false;
    assert!(session
        .job_start_marker(false, StartMode::AktuellePosition, Anchor::Center)
        .is_none());
}

#[test]
fn muster_fuellung_wandert_beim_verschieben_der_quelle_mit() {
    // Nutzerbefund: „Objekt mit Patternfill verschieben — Füllung bleibt
    // stehen." Muster-Shapes sind eigenständig; seit dem Fix gruppieren sie
    // sich mit der Quelle, und die Gruppenauswahl nimmt sie beim Move mit.
    let mut session = session_with_rect(); // Rect (0,0)–(10,10)
    session.selected = vec![0];
    session
        .pattern_fill(&luxifer_core::pattern_fill::FillParams::default())
        .expect("Füllung");
    let muster: Vec<usize> = (1..session.state().shapes.len()).collect();
    assert!(!muster.is_empty());

    // Quelle und Muster teilen eine Gruppe.
    let gid = session.state().shapes[0]
        .group_id
        .expect("Quelle gruppiert");
    for &i in &muster {
        assert_eq!(session.state().shapes[i].group_id, Some(gid));
    }

    // Klick auf die Quelle expandiert zur Gruppe …
    session.clear_selection();
    session.select_at(5.0, 5.0, 0.5, false);
    for &i in &muster {
        assert!(
            session.state().selected.contains(&i),
            "Muster-Shape {i} muss mitselektiert sein"
        );
    }

    // … und die Move-Geste verschiebt Muster UND Quelle.
    let before = session.state().shapes[muster[0]].bbox();
    session.begin_edit();
    session.translate_edit(50.0, 0.0);
    session.commit_edit();
    let after = session.state().shapes[muster[0]].bbox();
    assert!(
        (after.x - before.x - 50.0).abs() < 1e-9,
        "Muster wandert mit"
    );
}
