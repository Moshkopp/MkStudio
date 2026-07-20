//! Hardwarelose Tests des Laser-Dienstes mit einem Ruida-Fake-Profil (der
//! Treiber kompiliert Bytes, ohne ein Gerät zu erreichen).

use studio_core::geometry::Geo;
use studio_core::{
    Connection, DriverKind, JobAction, LaserProfile, LaserRegistry, SavedOrigin, StartReference,
};

use super::LaserService;

fn service_with_ruida() -> LaserService {
    service_with_ruida_at("192.168.1.100")
}

fn service_with_ruida_at(ip: &str) -> LaserService {
    let profile = LaserProfile {
        id: "test-ruida".into(),
        name: "Test-Ruida".into(),
        kind: DriverKind::Ruida,
        connection: Connection::Netz {
            ip: ip.into(),
            port: None,
        },
        bed_mm: (600.0, 400.0),
        ..Default::default()
    };
    let mut registry = LaserRegistry::default();
    registry.add(profile);
    registry.set_active("test-ruida");
    LaserService::with_registry(registry)
}

fn one_rect() -> (Vec<studio_core::Shape>, Vec<studio_core::Layer>) {
    let mut s = studio_core::AppState::new();
    s.add_shape(Geo::Rect {
        x: 10.0,
        y: 10.0,
        w: 50.0,
        h: 30.0,
    });
    (s.shapes.clone(), s.layers.clone())
}

#[test]
fn preview_bleibt_bei_relativen_referenzen_an_projektposition() {
    let svc = service_with_ruida();
    let (shapes, layers) = one_rect();
    let absolute = svc
        .execution_trace(&shapes, &layers, &StartReference::Absolut, 4)
        .unwrap();
    let current = svc
        .execution_trace(&shapes, &layers, &StartReference::AktuellePosition, 0)
        .unwrap();
    let user_origin = svc
        .execution_trace(&shapes, &layers, &StartReference::Benutzerursprung, 8)
        .unwrap();

    assert_eq!(current, absolute);
    assert_eq!(user_origin, absolute);
}

/// Dienst mit Ruida-Profil samt gespeichertem Nullpunkt (ADR 0020).
fn service_with_saved_origin() -> LaserService {
    let mut svc = service_with_ruida();
    let mut profile = svc.registry.active().unwrap().clone();
    profile.saved_origins = vec![SavedOrigin {
        id: "origin-1".into(),
        name: "Untersetzer Posi".into(),
        x_mm: 100.0,
        y_mm: 50.0,
    }];
    svc.registry.update(profile);
    svc
}

#[test]
fn gespeicherter_nullpunkt_verschiebt_den_job_auf_die_zielkoordinate() {
    let svc = service_with_saved_origin();
    let (shapes, layers) = one_rect();
    // Anker NW (Index 0): das Rechteck (10..60, 10..40) soll mit seiner
    // linken oberen Ecke auf (100, 50) liegen.
    let reference = StartReference::GespeicherterNullpunkt {
        id: "origin-1".into(),
    };
    let placed = svc
        .execution_trace(&shapes, &layers, &reference, 0)
        .unwrap();
    let absolute = svc
        .execution_trace(&shapes, &layers, &StartReference::Absolut, 0)
        .unwrap();
    assert_ne!(placed, absolute, "Nullpunkt-Referenz verschiebt den Job");
    let min = placed.moves.iter().fold((f64::MAX, f64::MAX), |acc, mv| {
        (
            acc.0.min(mv.to.0).min(mv.from.0),
            acc.1.min(mv.to.1).min(mv.from.1),
        )
    });
    assert!((min.0 - 100.0).abs() < 1e-6, "min_x {} statt 100", min.0);
    assert!((min.1 - 50.0).abs() < 1e-6, "min_y {} statt 50", min.1);
}

#[test]
fn fehlender_oder_ungueltiger_nullpunkt_hat_keinen_stillen_fallback() {
    let mut svc = service_with_saved_origin();
    let (shapes, layers) = one_rect();
    let missing = StartReference::GespeicherterNullpunkt {
        id: "geloescht".into(),
    };
    let err = svc
        .execution_trace(&shapes, &layers, &missing, 4)
        .unwrap_err();
    assert_eq!(err.code(), "origin_missing");
    let err = svc
        .export_to(
            std::path::Path::new("/tmp/none.rd"),
            &shapes,
            &layers,
            &missing,
            4,
        )
        .unwrap_err();
    assert_eq!(err.code(), "origin_missing");

    // Bett schrumpft unter die gespeicherte Koordinate → Eintrag ungültig.
    let mut profile = svc.registry.active().unwrap().clone();
    profile.bed_mm = (80.0, 40.0);
    svc.registry.update(profile);
    let reference = StartReference::GespeicherterNullpunkt {
        id: "origin-1".into(),
    };
    let err = svc
        .execution_trace(&shapes, &layers, &reference, 4)
        .unwrap_err();
    assert_eq!(err.code(), "origin_invalid");
    let err = svc.move_to_saved_origin("origin-1", 100.0).unwrap_err();
    assert_eq!(err.code(), "origin_invalid");
}

#[test]
fn nullpunkt_aktionen_verlangen_verbindung_und_gueltige_namen() {
    let mut svc = service_with_saved_origin();
    // Speichern liest die Position frisch — ohne Verbindung gesperrt.
    let err = svc
        .save_current_position_as_origin("Neue Posi")
        .unwrap_err();
    assert_eq!(err.code(), "laser_not_connected");
    // Statuslesen ebenso.
    assert_eq!(svc.read_status().unwrap_err().code(), "laser_not_connected");
    assert_eq!(
        svc.read_user_origin().unwrap_err().code(),
        "laser_not_connected"
    );
    // Anfahren prüft die Bettgrenzen VOR jedem Treiberkontakt.
    let err = svc.move_to_position(9_999.0, 0.0, 100.0).unwrap_err();
    assert_eq!(err.code(), "move_out_of_bed");
    let err = svc.move_to_position(f64::NAN, 0.0, 100.0).unwrap_err();
    assert_eq!(err.code(), "move_out_of_bed");
}

#[test]
fn ursprung_anfahren_folgt_der_gewaehlten_referenz() {
    let mut svc = service_with_saved_origin();
    // „Aktuelle Position": nichts anzufahren — kein Treiberkontakt nötig.
    let message = svc
        .goto_reference(&StartReference::AktuellePosition, 100.0)
        .unwrap();
    assert!(message.contains("keine Bewegung"), "{message}");
    // Alle bewegenden Varianten verlangen die ausdrückliche Verbindung.
    for reference in [
        StartReference::Absolut,
        StartReference::Benutzerursprung,
        StartReference::GespeicherterNullpunkt {
            id: "origin-1".into(),
        },
    ] {
        let err = svc.goto_reference(&reference, 100.0).unwrap_err();
        assert_eq!(err.code(), "laser_not_connected", "{reference:?}");
    }
    // Fehlender Nullpunkt bleibt ein harter Fehler ohne Fallback.
    let err = svc
        .goto_reference(
            &StartReference::GespeicherterNullpunkt {
                id: "geloescht".into(),
            },
            100.0,
        )
        .unwrap_err();
    assert_eq!(err.code(), "origin_missing");
}

#[test]
fn nullpunkte_umbenennen_und_loeschen_pflegen_die_liste() {
    let _g = crate::test_env::with_temp_dir("laser_saved_origins");
    let mut svc = service_with_saved_origin();
    // Leerer Name wird abgelehnt.
    let err = svc.rename_saved_origin("origin-1", "   ").unwrap_err();
    assert_eq!(err.code(), "origin_name");
    // Umbenennen ändert die ID nicht.
    svc.rename_saved_origin("origin-1", "Halterung").unwrap();
    let profile = svc.registry.active().unwrap();
    assert_eq!(profile.saved_origin("origin-1").unwrap().name, "Halterung");
    // Version wird beim Schreiben auf die höchste verstandene gestempelt.
    assert_eq!(
        profile.schema_version,
        studio_core::LASER_PROFILE_SCHEMA_VERSION
    );
    // Unbekannte ID ist ein Fehler, kein stilles Nichtstun.
    let err = svc.rename_saved_origin("fehlt", "X").unwrap_err();
    assert_eq!(err.code(), "origin_missing");
    // Löschen entfernt den Eintrag.
    svc.delete_saved_origin("origin-1").unwrap();
    assert!(svc.registry.active().unwrap().saved_origins.is_empty());
}

#[test]
fn empfangener_alter_stand_ueberschreibt_keine_lokale_pending_aenderung() {
    // Nutzerbefund „Nullpunkt erscheint erst nach Neustart": Der Sync-Worker
    // liefert zyklisch den vollen Katalogstand. Ein Datensatz aus einem Zyklus
    // VOR dem lokalen Speichern darf die frisch gespeicherte Nullpunktliste
    // nicht zurücksetzen, solange die Änderung in der Outbox liegt.
    let _g = crate::test_env::with_temp_dir("laser_pending_guard");
    let mut svc = service_with_ruida();
    let old_profile = svc.registry.active().unwrap().clone();
    // Lokale Änderung: Nullpunkt speichern (legt einen Pending-Eintrag an).
    svc.add_saved_origin("Untersetzer", 100.0, 50.0).unwrap();
    assert_eq!(svc.registry.active().unwrap().saved_origins.len(), 1);
    // Jetzt trifft der ALTE Stand (ohne Nullpunkt) aus dem vorherigen
    // Sync-Zyklus ein — er darf nicht angewendet werden.
    let record = crate::SharedCatalogRecord {
        sequence: 1,
        kind: crate::CatalogKind::LaserProfile,
        id: old_profile.id.clone(),
        deleted: false,
        content_hash: "x".into(),
        payload: Some(serde_json::to_string(&old_profile).unwrap()),
        workplace_id: "office".into(),
        changed_at_unix: 0,
    };
    assert!(!svc.apply_shared_record(&record).unwrap());
    assert_eq!(
        svc.registry.active().unwrap().saved_origins.len(),
        1,
        "lokale Änderung bleibt erhalten"
    );
}

#[test]
fn empfangenes_profil_mit_neuerer_schemaversion_wird_nicht_uebernommen() {
    let _g = crate::test_env::with_temp_dir("laser_schema_guard");
    let mut svc = service_with_ruida();
    let mut profile = svc.registry.active().unwrap().clone();
    profile.schema_version = 99;
    let record = crate::SharedCatalogRecord {
        sequence: 1,
        kind: crate::CatalogKind::LaserProfile,
        id: profile.id.clone(),
        deleted: false,
        content_hash: "x".into(),
        payload: Some(serde_json::to_string(&profile).unwrap()),
        workplace_id: "office".into(),
        changed_at_unix: 0,
    };
    let err = svc.apply_shared_record(&record).unwrap_err();
    assert_eq!(err.code(), "catalog_schema");
    // Der lokale Stand bleibt unangetastet.
    assert_eq!(
        svc.registry.active().unwrap().schema_version,
        studio_core::LASER_PROFILE_SCHEMA_VERSION
    );
}

#[test]
fn aktiver_ruida_meldet_aktionen() {
    let svc = service_with_ruida();
    let actions = svc.actions();
    assert!(!actions.is_empty(), "Ruida sollte Aktionen melden");
    assert!(actions.iter().any(|a| matches!(a, JobAction::SendJob)));
    assert!(actions.iter().any(|a| matches!(a, JobAction::ExportFile)));
}

#[test]
fn ohne_aktiven_laser_meldet_stabilen_fehler() {
    let mut svc = LaserService::with_registry(LaserRegistry::default());
    assert!(svc.actions().is_empty());
    let (shapes, layers) = one_rect();
    let err = svc
        .export_to(
            std::path::Path::new("/tmp/none.rd"),
            &shapes,
            &layers,
            &StartReference::Absolut,
            4,
        )
        .unwrap_err();
    assert_eq!(err.code(), "no_active_laser");
}

#[test]
fn export_erzeugt_ruida_bytes() {
    let mut svc = service_with_ruida();
    let (shapes, layers) = one_rect();
    let tmp = std::env::temp_dir().join(format!("studio_test_job_{}.rd", std::process::id()));
    svc.export_to(&tmp, &shapes, &layers, &StartReference::Absolut, 4)
        .expect("Export sollte klappen");
    let bytes = std::fs::read(&tmp).unwrap();
    assert!(!bytes.is_empty(), "Ruida-Job darf nicht leer sein");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn plan_rastert_bild_assets_wie_die_vorschau() {
    // Der echte Job muss dieselben Bilder rastern, die die Vorschau zeigt —
    // gleicher Resolver (assets::resolve_luma), prozessglobales Datenverzeichnis.
    let _g = crate::test_env::with_temp_dir("laser_plan_raster");
    let png = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../native/tests/fixtures/test2x2.png"
    ));
    let Ok(bytes) = png else {
        eprintln!("Fixture fehlt — Test übersprungen");
        return;
    };
    let meta = studio_core::import_image(&studio_core::assets_dir(), &bytes, "test.png")
        .expect("import_image");

    let mut s = studio_core::AppState::new();
    s.add_image(meta.id.clone(), 0.0, 0.0, 10.0, 10.0);
    let plan = service_with_ruida().plan(&s.shapes, &s.layers);

    let has_raster = plan.layers.iter().any(|l| {
        matches!(
            &l.work,
            studio_core::LayerWork::Raster { rows, texture } if !rows.is_empty() && texture.is_some()
        )
    });
    assert!(has_raster, "Bild-Layer wird im JobPlan gerastert");
}

#[test]
fn verbindungspflicht_ist_korrekt_klassifiziert() {
    // Export kompiliert nur (kein Gerät nötig); alles andere fährt die Maschine.
    assert!(!super::needs_connection(JobAction::ExportFile));
    for a in [
        JobAction::SendJob,
        JobAction::StreamGcode,
        JobAction::Frame,
        JobAction::RubberFrame,
        JobAction::Pause,
        JobAction::Stop,
        JobAction::Home,
        JobAction::GoOrigin,
    ] {
        assert!(super::needs_connection(a), "{a:?} braucht Verbindung");
    }
}

#[test]
fn run_action_verlangt_explizite_verbindung() {
    let mut svc = service_with_ruida_at("127.0.0.1");
    let (shapes, layers) = one_rect();
    let err = svc
        .run_action(
            JobAction::Frame,
            &shapes,
            &layers,
            &StartReference::Absolut,
            4,
        )
        .unwrap_err();
    assert_eq!(err.code(), "laser_not_connected");
}

#[test]
fn explizites_verbinden_meldet_ziel_und_ursache() {
    let mut svc = service_with_ruida_at("127.0.0.1");
    let err = svc.connect().unwrap_err();
    assert_eq!(err.code(), "laser_connect");
    assert!(err.message().contains("127.0.0.1"), "Ziel in der Meldung");
    assert!(err.details().is_some(), "technische Ursache vorhanden");
    assert!(!svc.is_connected());
}

/// Simuliert eine bestehende Verbindung zum aktiven Profil ohne Hardware:
/// Treiber gebaut und als verbunden markiert (Tests sind Kindmodul und dürfen
/// die privaten Felder setzen).
fn mark_connected(svc: &mut LaserService) {
    let profile = svc.registry.active().unwrap().clone();
    svc.driver = Some(std::sync::Arc::new(std::sync::Mutex::new(
        super::driver_for(&profile),
    )));
    svc.driver_id = Some(profile.id.clone());
    svc.connected_id = Some(profile.id);
}

#[test]
fn profil_speichern_ohne_verbindungsaenderung_haelt_die_verbindung() {
    // Nutzerbefund: Profil speichern (z. B. neuer Nullpunkt oder Name)
    // beendete grundlos die Laser-Verbindung.
    let _g = crate::test_env::with_temp_dir("laser_save_keeps_connection");
    let mut svc = service_with_ruida();
    mark_connected(&mut svc);
    let mut profile = svc.registry.active().unwrap().clone();
    profile.name = "Umbenannt".into();
    profile.bed_mm = (900.0, 600.0);
    profile.saved_origins = vec![SavedOrigin {
        id: "origin-1".into(),
        name: "Halterung".into(),
        x_mm: 10.0,
        y_mm: 10.0,
    }];
    svc.save_profile(profile).unwrap();
    assert!(
        svc.is_connected(),
        "Speichern ohne Verbindungsänderung trennt nicht"
    );
}

#[test]
fn profil_speichern_mit_neuem_verbindungsziel_trennt() {
    let _g = crate::test_env::with_temp_dir("laser_save_new_target_disconnects");
    let mut svc = service_with_ruida();
    mark_connected(&mut svc);
    let mut profile = svc.registry.active().unwrap().clone();
    profile.connection = Connection::Netz {
        ip: "10.0.0.7".into(),
        port: None,
    };
    svc.save_profile(profile).unwrap();
    assert!(
        !svc.is_connected(),
        "neues Verbindungsziel erzwingt Neuverbindung"
    );
}

#[test]
fn fremdes_profil_speichern_oder_loeschen_haelt_die_verbindung() {
    let _g = crate::test_env::with_temp_dir("laser_other_profile_keeps_connection");
    let mut svc = service_with_ruida();
    mark_connected(&mut svc);
    let other = LaserProfile {
        id: "zweiter".into(),
        name: "Zweiter Laser".into(),
        ..Default::default()
    };
    svc.save_profile(other).unwrap();
    assert!(svc.is_connected(), "fremdes Profil speichern trennt nicht");
    svc.delete_profile("zweiter").unwrap();
    assert!(svc.is_connected(), "fremdes Profil löschen trennt nicht");
    svc.delete_profile("test-ruida").unwrap();
    assert!(!svc.is_connected(), "das verbundene Gerät löschen trennt");
}

#[test]
fn geaenderte_achsen_bauen_den_treiber_neu() {
    // Die Inversion steckt über from_profile im Treiber. Ohne Neuaufbau bliebe
    // nach dem Umschalten der Checkbox die alte Richtung aktiv — die Achse
    // führe weiter verkehrt herum.
    let _g = crate::test_env::with_temp_dir("laser_axes_invalidate_driver");
    let mut svc = service_with_ruida();
    mark_connected(&mut svc);
    let mut profile = svc.registry.active().unwrap().clone();
    profile.axes.invert_u = !profile.axes.invert_u;
    svc.save_profile(profile).unwrap();
    assert!(
        !svc.is_connected(),
        "geänderte Achsen-Inversion muss den Treiber neu bauen"
    );
}

#[test]
fn geaenderter_rotary_baut_den_treiber_neu() {
    let _g = crate::test_env::with_temp_dir("laser_rotary_invalidates_driver");
    let mut svc = service_with_ruida();
    mark_connected(&mut svc);
    let mut profile = svc.registry.active().unwrap().clone();
    profile.rotary = Some(studio_core::Rotary::default());
    svc.save_profile(profile).unwrap();
    assert!(
        !svc.is_connected(),
        "geänderter Rotary muss den Treiber neu bauen"
    );
}

#[test]
fn nicht_eingerichtete_achse_wird_nicht_angefahren() {
    // Sicherheitsregel in der Application, nicht nur in der UI: ein Shortcut
    // oder eine andere Ansicht darf eine fehlende Achse nicht bewegen.
    let _g = crate::test_env::with_temp_dir("laser_axis_gate");
    let mut svc = service_with_ruida();
    mark_connected(&mut svc);
    let error = svc
        .jog_axis(
            studio_core::MachineAxis::Z,
            studio_core::AxisDir::Forward,
            studio_core::JogMotion::Step(1.0),
            10.0,
        )
        .expect_err("Z ohne has_z_axis muss abgelehnt werden");
    assert_eq!(error.code(), "laser_axis_unavailable");

    let error = svc
        .jog_axis(
            studio_core::MachineAxis::U,
            studio_core::AxisDir::Forward,
            studio_core::JogMotion::HoldStart,
            10.0,
        )
        .expect_err("U ohne has_u_axis muss abgelehnt werden");
    assert_eq!(error.code(), "laser_axis_unavailable");
}
