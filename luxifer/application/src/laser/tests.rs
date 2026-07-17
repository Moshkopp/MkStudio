//! Hardwarelose Tests des Laser-Dienstes mit einem Ruida-Fake-Profil (der
//! Treiber kompiliert Bytes, ohne ein Gerät zu erreichen).

use luxifer_core::geometry::Geo;
use luxifer_core::{Connection, DriverKind, JobAction, LaserProfile, LaserRegistry, StartMode};

use super::LaserService;

#[test]
fn benutzerursprung_verschiebt_alle_preview_koordinaten() {
    let mut builder = luxifer_core::TraceBuilder::new(false);
    builder.work(
        (1.0, 2.0),
        (3.0, 4.0),
        (0.9, 2.1),
        (2.9, 4.1),
        luxifer_core::ExecutionKind::Cut,
        0,
    );
    let mut trace = builder.finish();

    super::translate_trace(&mut trace, (100.0, 50.0));

    let movement = trace.moves[0];
    assert_eq!(movement.ideal_from, (101.0, 52.0));
    assert_eq!(movement.ideal_to, (103.0, 54.0));
    assert_eq!(movement.from, (100.9, 52.1));
    assert_eq!(movement.to, (102.9, 54.1));
}

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

fn one_rect() -> (Vec<luxifer_core::Shape>, Vec<luxifer_core::Layer>) {
    let mut s = luxifer_core::AppState::new();
    s.add_shape(Geo::Rect {
        x: 10.0,
        y: 10.0,
        w: 50.0,
        h: 30.0,
    });
    (s.shapes.clone(), s.layers.clone())
}

#[test]
fn aktiver_ruida_meldet_aktionen() {
    let mut svc = service_with_ruida();
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
            StartMode::Absolut,
            4,
        )
        .unwrap_err();
    assert_eq!(err.code(), "no_active_laser");
}

#[test]
fn export_erzeugt_ruida_bytes() {
    let mut svc = service_with_ruida();
    let (shapes, layers) = one_rect();
    let tmp = std::env::temp_dir().join(format!("luxifer_test_job_{}.rd", std::process::id()));
    svc.export_to(&tmp, &shapes, &layers, StartMode::Absolut, 4)
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
    let meta = luxifer_core::import_image(&luxifer_core::assets_dir(), &bytes, "test.png")
        .expect("import_image");

    let mut s = luxifer_core::AppState::new();
    s.add_image(meta.id.clone(), 0.0, 0.0, 10.0, 10.0);
    let plan = service_with_ruida().plan(&s.shapes, &s.layers);

    let has_raster = plan.layers.iter().any(|l| {
        matches!(
            &l.work,
            luxifer_core::LayerWork::Raster { rows, texture } if !rows.is_empty() && texture.is_some()
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
        .run_action(JobAction::Frame, &shapes, &layers, StartMode::Absolut, 4)
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
