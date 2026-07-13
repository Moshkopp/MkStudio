//! Roundtrip- und Fehlerpfad-Tests des Projektdienstes. Läuft gegen ein
//! temporäres Datenverzeichnis über `LUXIFER_DATA_DIR` (gemeinsamer Lock in
//! `crate::test_env`, weil die Env-Variable prozessglobal ist).

use luxifer_core::state::AppState;
use luxifer_core::Geo;

use super::ProjectService;
use crate::test_env::with_temp_dir;
use crate::{list_outbox, OutboxStatus};

fn state_with_rect() -> AppState {
    let mut s = AppState::new();
    s.add_shape(Geo::Rect {
        x: 5.0,
        y: 5.0,
        w: 30.0,
        h: 20.0,
    });
    s
}

#[test]
fn anlegen_speichern_oeffnen_roundtrip() {
    let _g = with_temp_dir("roundtrip");
    let mut svc = ProjectService::new();
    let state = state_with_rect();
    let n = state.shapes.len();

    svc.new_project(&state, "  Erstes  ", "").unwrap();
    // Name wird getrimmt.
    assert_eq!(svc.open_name(), Some("Erstes"));
    assert!(svc.has_open());

    // In der Liste sichtbar.
    assert!(svc.list().iter().any(|p| p.name == "Erstes"));

    // Frischer Dienst öffnet dasselbe Projekt und bekommt den Zustand zurück.
    let mut svc2 = ProjectService::new();
    let restored = svc2.open("Erstes").unwrap();
    assert_eq!(restored.shapes.len(), n);
    assert_eq!(svc2.open_name(), Some("Erstes"));
}

#[test]
fn schliessen_loest_nur_die_offene_projektbindung() {
    let _g = with_temp_dir("close_project");
    let mut svc = ProjectService::new();
    svc.new_project(&AppState::default(), "Bleibt", "").unwrap();

    svc.close();

    assert!(!svc.has_open());
    assert_eq!(svc.list().len(), 1);
    assert_eq!(svc.list()[0].name, "Bleibt");
}

#[test]
fn outbox_friert_jeden_speicherstand_mit_parent_ein() {
    let _g = with_temp_dir("outbox_chain");
    let mut svc = ProjectService::new();
    let mut state = state_with_rect();
    svc.new_project(&state, "Sync", "").unwrap();

    let first = svc.queue_current_for_sync("office-id").unwrap();
    let first_payload = std::fs::read(first.payload_path()).unwrap();
    assert_eq!(first.status, OutboxStatus::Pending);
    assert_eq!(first.parent_revision_id, None);

    state.add_shape(Geo::Ellipse {
        cx: 60.0,
        cy: 40.0,
        rx: 10.0,
        ry: 8.0,
    });
    svc.save(&state).unwrap();
    let second = svc.queue_current_for_sync("office-id").unwrap();

    assert_eq!(
        second.parent_revision_id.as_deref(),
        Some(first.revision_id.as_str())
    );
    assert_eq!(first.project_version_id, second.project_version_id);
    assert_ne!(first.content_hash, second.content_hash);
    assert_eq!(std::fs::read(first.payload_path()).unwrap(), first_payload);
    assert_eq!(list_outbox().unwrap().len(), 2);
    crate::sync_outbox::set_outbox_status(&first.revision_id, OutboxStatus::Uploaded, None)
        .unwrap();
    let persisted = list_outbox().unwrap();
    assert_eq!(persisted[0].status, OutboxStatus::Uploaded);
}

#[test]
fn beschreibung_wird_gespeichert_und_getrimmt() {
    let _g = with_temp_dir("description");
    let mut svc = ProjectService::new();
    svc.new_project(
        &state_with_rect(),
        "Beschrieben",
        "  Untersetzer aus Eiche  ",
    )
    .unwrap();
    let detail = svc.detail("Beschrieben").unwrap();
    assert_eq!(detail.description, "Untersetzer aus Eiche");
}

#[test]
fn leerer_name_wird_abgewiesen() {
    let _g = with_temp_dir("empty_name");
    let mut svc = ProjectService::new();
    let err = svc.new_project(&state_with_rect(), "   ", "").unwrap_err();
    assert_eq!(err.code(), "project_name_empty");
    assert!(!svc.has_open());
}

#[test]
fn speichern_ohne_offenes_projekt_liefert_fehler() {
    let _g = with_temp_dir("no_open");
    let mut svc = ProjectService::new();
    let err = svc.save(&state_with_rect()).unwrap_err();
    assert_eq!(err.code(), "no_open_project");
}

#[test]
fn oeffnen_unbekannt_laesst_bisheriges_projekt_erhalten() {
    let _g = with_temp_dir("open_unknown");
    let mut svc = ProjectService::new();
    svc.new_project(&state_with_rect(), "A", "").unwrap();

    let err = svc.open("gibt-es-nicht").unwrap_err();
    assert_eq!(err.code(), "project_read");
    // Fehler hält technische Ursache fest, ohne den Zustand zu verlieren.
    assert!(err.details().is_some());
    assert_eq!(svc.open_name(), Some("A"));
}

#[test]
fn version_anlegen_und_auflisten() {
    let _g = with_temp_dir("versions");
    let mut svc = ProjectService::new();
    let state = state_with_rect();
    svc.new_project(&state, "V", "").unwrap();
    let before = svc.versions().len();
    svc.save_version(&state).unwrap();
    assert_eq!(svc.versions().len(), before + 1);
}

#[test]
fn detail_und_peek_wechseln_nichts() {
    let _g = with_temp_dir("detail_peek");
    let mut svc = ProjectService::new();
    let state = state_with_rect();
    svc.new_project(&state, "A", "").unwrap();
    svc.new_project(&state, "B", "").unwrap();
    assert_eq!(svc.open_name(), Some("B"));

    // Detail eines NICHT offenen Projekts: nur lesen, offenes bleibt.
    let d = svc.detail("A").unwrap();
    assert_eq!(d.name, "A");
    assert_eq!(d.versions.len(), 1);
    assert_eq!(d.current_version, d.versions[0].id);
    assert_eq!(svc.open_name(), Some("B"));

    // Detail des offenen Projekts kommt aus dem Speicher.
    let d = svc.detail("B").unwrap();
    assert_eq!(d.name, "B");

    // Peek liefert den Zustand, ohne das offene Projekt zu wechseln.
    let peeked = svc.peek_state("A").unwrap();
    assert_eq!(peeked.shapes.len(), state.shapes.len());
    assert_eq!(svc.open_name(), Some("B"));

    // Unbekanntes Projekt scheitert sauber.
    assert_eq!(svc.detail("nix").unwrap_err().code(), "project_read");
    assert_eq!(svc.peek_state("nix").unwrap_err().code(), "project_read");
}

#[test]
fn version_loeschen_befoerdert_bei_aktueller() {
    let _g = with_temp_dir("version_delete");
    let mut svc = ProjectService::new();
    let v1_state = state_with_rect();
    svc.new_project(&v1_state, "V", "").unwrap();

    // Zweite Version mit zusätzlicher Form anlegen; sie wird die aktuelle.
    let mut v2_state = state_with_rect();
    v2_state.add_shape(Geo::Rect {
        x: 50.0,
        y: 50.0,
        w: 10.0,
        h: 10.0,
    });
    svc.save_version(&v2_state).unwrap();
    let v2_id = svc.current_version_id().unwrap().to_string();
    let v1_id = svc
        .versions()
        .iter()
        .find(|v| v.id != v2_id)
        .unwrap()
        .id
        .clone();

    // Nicht-aktuelle Version löschen: kein Zustandswechsel nötig.
    svc.save_version(&v2_state).unwrap(); // dritte Version, damit V1 löschbar
    assert!(svc.delete_version(&v1_id).unwrap().is_none());

    // Aktuelle Version löschen: der beförderte Zustand kommt zurück.
    let current = svc.current_version_id().unwrap().to_string();
    let promoted = svc.delete_version(&current).unwrap();
    assert!(promoted.is_some());

    // Letzte Version ist geschützt.
    let last = svc.current_version_id().unwrap().to_string();
    assert_eq!(
        svc.delete_version(&last).unwrap_err().code(),
        "version_delete"
    );
}

#[test]
fn umbenennen_und_loeschen() {
    let _g = with_temp_dir("rename_delete");
    let mut svc = ProjectService::new();
    svc.new_project(&state_with_rect(), "Alt", "").unwrap();

    svc.rename("Alt", "Neu").unwrap();
    assert_eq!(svc.open_name(), Some("Neu"));
    assert!(svc.list().iter().any(|p| p.name == "Neu"));
    assert!(!svc.list().iter().any(|p| p.name == "Alt"));

    // Leerer neuer Name wird abgewiesen.
    let err = svc.rename("Neu", "  ").unwrap_err();
    assert_eq!(err.code(), "project_name_empty");

    // Löschen des offenen Projekts schließt es.
    svc.delete("Neu").unwrap();
    assert!(!svc.has_open());
    assert!(!svc.list().iter().any(|p| p.name == "Neu"));
}

#[test]
fn export_kopiert_projektdatei() {
    let _g = with_temp_dir("export");
    let mut svc = ProjectService::new();
    svc.new_project(&state_with_rect(), "Exp", "").unwrap();

    let ziel = std::env::temp_dir().join(format!("luxifer_export_{}.luxi", std::process::id()));
    let _ = std::fs::remove_file(&ziel);
    svc.export("Exp", &ziel).unwrap();
    assert!(ziel.exists());
    let _ = std::fs::remove_file(&ziel);

    // Export eines unbekannten Projekts scheitert sauber.
    let err = svc.export("gibt-es-nicht", &ziel).unwrap_err();
    assert_eq!(err.code(), "project_export");
}
