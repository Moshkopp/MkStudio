//! Hardwarelose Tests des Laser-Dienstes mit einem Ruida-Fake-Profil (der
//! Treiber kompiliert Bytes, ohne ein Gerät zu erreichen).

use luxifer_core::geometry::Geo;
use luxifer_core::{Connection, DriverKind, JobAction, LaserProfile, LaserRegistry, StartMode};

use super::LaserService;

fn service_with_ruida() -> LaserService {
    let profile = LaserProfile {
        id: "test-ruida".into(),
        name: "Test-Ruida".into(),
        kind: DriverKind::Ruida,
        connection: Connection::Netz {
            ip: "192.168.1.100".into(),
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
