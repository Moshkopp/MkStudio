# Native-only-Migration: Arbeits- und Übergabeliste

Stand: 2026-07-12  
Architekturentscheidung: [ADR 0011](adr/0011-native-only-anwendungsschicht-und-tauri-abbau.md)

## Zweck und Arbeitsregel

Diese Datei ist die fortsetzbare Quelle der Wahrheit für den Wechsel von
Svelte/Tauri zu einer ausschließlich nativen LuxiFer-Anwendung. Jeder Agent
liest vor Änderungen zuerst ADR 0010, ADR 0011, diese Datei und danach nur die
für den nächsten offenen Schnitt genannten Quelldateien.

Die vollständige Command- und UI-Inventur wird in
[`docs/native_function_matrix.md`](native_function_matrix.md) fortgeführt.

Nach jedem stabilen Arbeitspaket:

- Checkboxen und Befunde hier aktualisieren;
- ausgeführte Prüfungen und bekannte Abweichungen eintragen;
- `git status --short` prüfen und fremde Änderungen nicht einbeziehen;
- den Schnitt als eigenen Commit abschließen, sofern Git-Schreibzugriff
  verfügbar und der Nutzer mit Commit-orientierter Umsetzung fortfährt.

Kein Punkt wird als erledigt markiert, solange nur die UI sichtbar ist. Erledigt
bedeutet: vollständiger Erfolgs- und Fehlerpfad, Tests, Native-Anbindung und
entfernte oder ausdrücklich verbliebene Altimplementierung.

## Aktueller Ausgangszustand

- [x] Nativer `winit + wgpu + egui`-Spike rendert performant.
- [x] `luxifer-core` wird nativ direkt gelinkt.
- [ ] Native ist funktional gleichwertig zur bisherigen Anwendung.
- [ ] Tauri-unabhängige Anwendungsschicht existiert.
- [ ] Tauri/Svelte ist entfernt.

Bekannte Arbeitskopie zu Beginn dieser Liste: uncommittete Änderungen in
`luxifer/native/src/app.rs`, `main.rs`, `tools.rs`, `ui.rs` sowie die neue Datei
`icons.rs`. Diese Änderungen gehören dem Nutzer und dürfen bei Architekturarbeit
nicht überschrieben oder ungeprüft in einen Commit aufgenommen werden.

## Definition of Done der Gesamtmigration

- [ ] Native startet zuverlässig mit leerem und bestehendem Projekt.
- [ ] Alle als erforderlich klassifizierten Tauri-Funktionen besitzen einen
      getesteten Native-/Application-Pfad.
- [ ] Alle nicht übernommenen Funktionen sind bewusst als `entfällt` begründet.
- [ ] Keine produktive Anwendungslogik liegt mehr unter `frontend/src-tauri`.
- [ ] Native enthält keine zweite Projekt-, Asset-, Geometrie- oder
      Laser-Fachimplementierung.
- [ ] Fehler werden ohne Panic über ein gemeinsames `AppError` angezeigt.
- [ ] Speichern, Schließen und Projektwechsel schützen ungespeicherte Änderungen.
- [ ] Große Referenzdatei lädt und bleibt beim Pan/Zoom flüssig.
- [ ] Core-, Application-, Native- und Treibertests sind grün.
- [ ] Release-Build der nativen Anwendung ist grün.
- [ ] Svelte, Tauri, WebView und zugehörige Buildkonfiguration sind gelöscht.
- [ ] README, ADR-Index, Roadmap und Startanweisungen beschreiben nur Native.

## Phase 0 — Bestand sichern und Wahrheit herstellen

Ziel: Keine weitere scheinbare Funktionsbreite; belastbare Migrationsmatrix.

- [ ] Aktuelle Native-Änderungen prüfen und als eigenen Spike-Checkpoint sichern.
- [ ] Native-Demodaten aus `App::new` entfernen oder klar hinter einen
      Entwicklungsmodus stellen.
- [ ] Alle sichtbaren Native-Aktionen inventarisieren.
- [ ] Nicht implementierte Aktionen deaktivieren und mit Tooltip
      „Noch nicht migriert“ kennzeichnen.
- [ ] Fehlerhafte Aktionen entweder reparieren oder bis zu ihrem Schnitt
      deaktivieren.
- [x] Tauri-Commands auf Funktionsebene vollständig inventarisieren:
  - [x] `frontend/src-tauri/src/lib.rs`
  - [x] `commands/shapes.rs`
  - [x] `commands/edit.rs`
  - [x] `commands/image.rs`
  - [x] `commands/project.rs`
  - [x] `commands/laser.rs`
- [ ] Für jeden Command eine Zeile in der Funktionsmatrix ergänzen:

| Bereich | Funktion | Quelle heute | Ziel | Status | Entscheidung/Abnahme |
|---|---|---|---|---|---|
| Editor | Auswahl/Hit-Test | Core + Tauri + Native | Core/Application | prüfen | ThorBurn-Regeln, additiv, leerer Klick |
| Editor | Undo/Redo | Core + beide UIs | Application | prüfen | Zustand und Dirty-Flag korrekt |
| Projekt | Öffnen/Speichern/Version | Tauri + Native-Duplikat | Application | prüfen | Assets/Metadaten/Fehler vollständig |
| Import | SVG/DXF | Core + Adapter | Application | prüfen | Dateifehler und große Datei |
| Import | Bild | Core + Adapter | Application | prüfen | Asset-Lebenszyklus und Parameter |
| Text | Font/Text→Pfad | Core + Adapter | Application | prüfen | Fontfehler, Metadaten, Editieren |
| Laser | Profile/Aktionen/Job | Tauri + Native-Duplikat | Application | prüfen | kein Hardwarezugriff im UI |

Detailmatrix: [`docs/native_function_matrix.md`](native_function_matrix.md).
Die Command-Zeilen und ersten Zielzuordnungen sind vollständig; Status und
Abnahmebefunde werden pro vertikalem Schnitt fortgeschrieben.

Abnahme Phase 0:

- [ ] Jeder sichtbare Native-Button ist `funktioniert`, `deaktiviert` oder
      `bewusst entfällt`; es gibt keine Attrappen.
- [ ] Jeder Tauri-Command ist einer Zielverantwortung zugeordnet.

## Phase 1 — `luxifer-application` als Grenze einführen

Ziel: Testbare Sitzung und konsistenter Aufrufpfad vor weiterer Migration.

- [x] Workspace-Crate `luxifer/application` anlegen und in `Cargo.toml`
      aufnehmen.
- [x] Abhängigkeiten nur in zulässiger Richtung aufbauen:
      `native -> application -> core/drivers`; niemals zurück.
- [ ] `Application` beziehungsweise fachlich geschnittene Services definieren
      (begonnen mit `EditorSession`; Projekt/Assets/Laser folgen schnittweise).
- [x] `EditorSession` mit eindeutigem Besitz des laufenden `AppState` einführen.
- [x] Einheitliches `AppError` definieren:
  - [x] stabiler Fehlercode;
  - [x] nutzerlesbare Meldung;
  - [x] optionale technische Ursache/Quelle;
  - [ ] Konvertierungen für I/O, Projektformat, Import und Treiberfehler.
- [x] Ergebnis-/Statusmodelle UI-unabhängig halten; keine `egui`, `winit`,
      `wgpu` oder Tauri-Typen.
- [ ] Native besitzt genau eine zentrale Fehleranzeige und loggt technische
      Details (Banner für `AppError` vorhanden; technisches Logging folgt mit
      den ersten I/O-Fehlerkonvertierungen).
- [x] Erste Application-Tests für Sitzung, Fehler und Dirty-Status ergänzen.

Abnahme Phase 1:

- [x] Eine triviale Operation (Löschen sowie Undo/Redo) läuft aus
      Native ausschließlich über Application.
- [x] Application-Tests laufen ohne Fenster/GPU.
- [x] `cargo test --workspace` ist grün.

Validierung des ersten Schnitts (2026-07-12): `cargo fmt --all -- --check`,
`cargo test --workspace` (239 Tests) und
`cargo clippy --workspace --all-targets --all-features -- -D warnings` grün.
`EditorSession` bietet vorübergehend `Deref`/`DerefMut` als explizit
dokumentierte Migrationsbrücke; neue Schnitte müssen benannte Session-/Service-
Methoden ergänzen und den Direktzugriff schrittweise verkleinern.

Strukturkorrektur 2026-07-12: Nach den ersten Schnitten war `session.rs` auf
600 Zeilen und 29 öffentliche Methoden angewachsen. Der Monolith wurde ohne
API-/Verhaltensänderung nach Verantwortung in `session/{selection,drawing,
actions,layers}.rs` zerlegt; Tests liegen separat in `session/tests.rs`. Der
Session-Root besitzt nur noch Zustand/Lebenszyklus, Undo/Redo, gemeinsame
Invarianten und die ausdrücklich temporäre Migrationsbrücke. Neue Features
müssen in das zuständige Modul oder einen eigenen Service, nicht zurück in den
Root.

## Phase 2 — Editor-Grundworkflow vollständig migrieren

Ziel: Ein kleiner, ehrlicher Editor, der zuverlässig benutzt werden kann.

- [ ] Szene lesen und Render-Invalidierung aus Application-Zustand ableiten.
- [x] Auswahl: Klick, additiv, Rechteckauswahl, Auswahl löschen.
- [x] Zeichnen: Rechteck, Ellipse, Linie, Polygon, Polylinie, Spline und Bézier
      einschließlich Abbruch und Abschluss.
- [x] Transformieren: Verschieben, Skalieren, proportional Skalieren, Rotieren
      und Spiegeln laufen über `EditorSession`.
- [ ] Transform-Handles und BBox ausschließlich aus kanonischer Core-Geometrie.
- [x] Layer/Farbe: Aktivieren, Sichtbarkeit, Job-Aktivierung, Sperre, Air Assist
      und Reihenfolge laufen über `EditorSession`; Parameterdialog und
      numerische Layerwerte (`set_layer_params` mit Validierung) sind migriert.
- [x] Löschen, Gruppieren, Aufheben, Undo und Redo laufen über
      `EditorSession`.
- [x] Tastaturkürzel einschließlich Fokusregeln für Textfelder/Dialoge
      (typisierte `Shortcut`-Zuordnung, Fokus-Gate über `wants_keyboard_input`).
- [x] Jede direkte Move-/Resize-/Rotate-Geste erzeugt genau einen sinnvollen
      Undo-Schritt.
- [x] Abbruch einer direkten Manipulationsgeste stellt den Ausgangszustand
      wieder her.
- [ ] Native-spezifische Geometrie-/Snapshot-Duplikate aus `app.rs` entfernen,
      sobald der Core/Application-Pfad sie ersetzt.

Abnahme Phase 2:

- [ ] Automatisierte Tests für Auswahl- und Transformregeln.
- [ ] Manueller Smoke-Test: zeichnen, mehrfach auswählen, bewegen, skalieren,
      rotieren, Farbe ändern, sperren, Undo/Redo, löschen.
- [ ] Keine bekannten Panics oder inkonsistenten Dirty-/Undo-Zustände.

Zwischenstand 2026-07-12: `EditorSession` kapselt Klick-/additive Auswahl,
Gruppenerweiterung, Marquee und den Gestenlebenszyklus
`begin_edit/commit_edit/cancel_edit`. Damit wurde ein Native-Fehler behoben:
Move-Drag legte zuvor keinen eigenen Undo-Punkt an, verwarf beim Loslassen aber
potenziell den letzten fremden Undo-Eintrag. Validierung: 242 Workspace-Tests
und Clippy mit `-D warnings` grün.

Zeichen-Schnitt 2026-07-12: `EditorSession` kapselt nun auch Boxformen, Linie,
Core-Polygonformen sowie punktbasierte Polylinie/Spline/Bézier-Pfade. Native
sammelt nur Werkzeugtyp und Weltpunkte. Mindestgrößen, Auswahl des Ergebnisses
und Undo liegen unterhalb der UI-Grenze; ungültige Mini-Gesten bleiben ohne
Mutation und ohne Undo-Eintrag. Validierung: 245 Workspace-Tests und Clippy mit
`-D warnings` grün.

Auswahloperationen-Schnitt 2026-07-12: Farbe, Spiegeln, Ausrichten, Verteilen,
Gruppieren, Nesting sowie die bereits sichtbaren Boolean-/Offset-/Fillet-
Aktionen laufen über benannte Session-Methoden. Auswahlvoraussetzungen liefern
`AppError`; nicht migrierte Aktionen nutzen `not_migrated` statt eines fremden
Laser-Statuskanals. Dabei wurde doppeltes Undo entfernt: Native setzte zuvor
zusätzliche Undo-Punkte vor Core-Operationen, die selbst bereits atomare Undo-
Punkte erzeugen. Validierung: 247 Workspace-Tests und Clippy mit `-D warnings`
grün.

Layer-Basisschnitt 2026-07-12: Die Native-Layerliste mutiert keine Layerfelder
mehr direkt. Sichtbarkeit, Job-Aktivierung, Sperre, Air Assist und Reihenfolge
laufen über validierte `EditorSession`-Methoden und sind Dirty-/Undo-fähig.
Reihenfolge nutzt weiterhin die kanonische Core-Remap-Operation für
`shape.layer_id`. Validierung: 250 Workspace-Tests und Clippy mit `-D warnings`
grün.

Layer-Parameterschnitt 2026-07-12: Der vollständige Parameterdialog
(Doppelklick auf eine Ebene) läuft über `EditorSession::set_layer_params`. Der
UI-unabhängige `LayerParams`-Typ nutzt den typisierten `LayerMode` statt eines
String-DTOs. Validiert werden Leistung im Prozentbereich, `min ≤ max`, positive
Geschwindigkeit (NaN gilt als ungültig), mindestens ein Durchlauf sowie die
Bild-Invariante (kein Wechsel Image↔Vektor). Zeilenabstand (Fill) und DPI
(Raster/Image) werden nur im jeweils relevanten Modus geprüft — genau die
Felder, die der Dialog zeigt; ein Cut-Layer mit altem `dpi = 0` bleibt so
speicherbar. Ungültige Werte liefern einen stabilen `AppError` ohne jede
Mutation (kein Dirty, kein zusätzlicher Undo-Punkt); ein gültiger Wechsel ist
genau ein Undo-Schritt. Native hält nur den Dialogentwurf; Speichern läuft über
die Session, Abbrechen verändert nichts. Validierung: 262 Workspace-Tests
(200 Core, 27 Application) und Clippy mit `-D warnings` grün.

Tastatur-/Fokusschnitt 2026-07-12: Die Canvas-Tastatur läuft über eine
typisierte `Shortcut`-Ebene (`tools::resolve_shortcut`), getrennt vom Auslösen,
und ist ohne winit/egui testbar. Das Fokus-Gate nutzt
`egui::Context::wants_keyboard_input`: hat ein Textfeld oder Dialog den
Tastaturfokus, feuert kein Canvas-Shortcut — Tippen hinter einem offenen
Layer-/Text-Dialog mutiert die Szene nicht mehr und wechselt kein Werkzeug.
Zusätzlich behoben: Undo/Redo verlangen jetzt Strg (ein nacktes „z"/„y" war
zuvor Undo/Redo). Die Ausführung (`App::apply_shortcut`) läuft weiter über die
`EditorSession`; die reine Taste→Aktion-Zuordnung bleibt Native-Präsentation.
Validierung: 267 Workspace-Tests (16 Native, davon 5 Shortcut-Tests) und Clippy
mit `-D warnings` grün; native App startet und rendert ohne Panic.

## Phase 3 — Projekt, Versionen und Assets

Ziel: Verlustfreies Arbeiten und vollständiger Datei-/Asset-Lebenszyklus.

- [ ] Tauri-Projektcommands und `native/src/project.rs` gegeneinander prüfen.
- [ ] Eine kanonische `ProjectService`-Implementierung in Application/Core
      herstellen; Duplikat entfernen.
- [ ] Neues Projekt, Liste, Öffnen, Speichern und „Neue Version“.
- [ ] Details, Umbenennen, Löschen, Import/Export von Projekten.
- [ ] Versionsliste, Version öffnen/löschen und Thumbnails.
- [ ] Asset-Verzeichnis und `asset_id`-Referenzen unverändert erhalten; keine
      Base64-Dauerablage.
- [ ] Autosave nur übernehmen, wenn der Workflow ausdrücklich festgelegt ist;
      sonst bewusst manuell speichern.
- [ ] Dirty-Guard bei Neu, Öffnen, Schließen und Programmende.
- [ ] Atomisches Speichern beziehungsweise sichere Fehlerbehandlung bei
      Teilfehlern prüfen.

Abnahme Phase 3:

- [ ] Roundtrip-Test mit Vektoren, Text und Bild-Asset.
- [ ] Versionswechsel verliert keine Assets oder Metadaten.
- [ ] Schreibfehler lässt den bisherigen Projektstand verwendbar.
- [ ] `native/src/project.rs` enthält keine konkurrierende Fachlogik mehr.

## Phase 4 — Import, Text und Bildbearbeitung

Ziel: Vollständige Erzeugungs- und Bearbeitungsworkflows statt Import-Demos.

- [ ] Nativen Dateidialog nur als Pfadlieferant behandeln.
- [ ] SVG- und DXF-Import inklusive Warnungen/Fehlern migrieren.
- [ ] Bildimport mit Asset-Anlage und Textur-Invalidierung migrieren.
- [ ] Bildparameter: Modus, Schwelle, Helligkeit, Kontrast, Gamma und Invert.
- [ ] Bildvorschau/Dithering ohne dauerhafte UI-Kopie des Assetzustands.
- [ ] Systemfonts auflisten, Textvorschau, Text anlegen und Text editieren.
- [ ] Fehlende/ungültige Fonts und nicht unterstützte Dateien verständlich
      behandeln.
- [ ] Trace-Workflow vollständig migrieren.

Abnahme Phase 4:

- [ ] Referenz-SVG/DXF/Bild/Text lassen sich importieren, speichern, erneut
      öffnen und bearbeiten.
- [ ] Abbruch im Dateidialog verändert das Projekt nicht.
- [ ] Fehler erzeugen keine leeren Layer, verwaisten Assets oder Undo-Leichen.

## Phase 5 — Geometrie- und Arrange-Werkzeuge

Ziel: Alle produktiv benötigten Operationen mit expliziten Voraussetzungen.

- [x] Ausrichten und Verteilen inklusive Gruppen über Application/Core.
- [x] Gruppieren/Aufheben und Spiegeln über Application/Core.
- [ ] Boolean: Vereinigung, Schnitt und Differenz.
- [ ] Offset und Fillet.
- [ ] Bridge und Ecken-Fillet.
- [ ] Nesting und Nest-Fill.
- [ ] Pattern Fill und Coaster-Einfügen.
- [ ] Bézier/Spline: Anlegen, Segment-Hit-Test, Knoten teilen/löschen,
      glatt/eckig und Handles ziehen.
- [ ] Aktionen bei ungeeigneter Auswahl deaktivieren; Grund per Tooltip oder
      Statusmeldung erklären.

Abnahme Phase 5:

- [ ] Ergebnis- und Regressionstests liegen überwiegend im Core.
- [ ] Native prüft nicht selbst geometrische Voraussetzungen nach.
- [ ] Jede mutierende Operation ist Undo/Redo-fähig.

## Phase 6 — Vorschau, Job und Laser

Ziel: Sicherer durchgängiger Weg vom Design zur Maschine.

- [ ] Jobparameter und Jobvorschau vollständig aus Core/Application beziehen.
- [ ] Native GPU-Vorschau für Cut/Fill/Raster/Image implementieren.
- [ ] Vorschau-Simulation und Monitorzustand festlegen und umsetzen.
- [ ] Tauri-Lasercommands und `native/src/laser.rs` inventarisieren.
- [ ] Ein kanonischer `LaserService` in Application:
  - [ ] Registry laden/speichern;
  - [ ] Profile anlegen/bearbeiten/löschen/aktivieren;
  - [ ] verfügbare Aktionen abfragen;
  - [ ] Ping/Verbindung/Position;
  - [ ] Start, Pause, Fortsetzen, Stopp, Frame und Export;
  - [ ] Jog und Home;
  - [ ] Fehler und Verbindungsabbruch.
- [ ] UI darf niemals direkt einen Ruida-/GRBL-Treiber erzeugen.
- [ ] Gefährliche Aktionen benötigen klare Zustände, Sperren und Rückmeldung.
- [ ] Hardwarelose Tests mit Fake-/Testtreiber ergänzen.

Abnahme Phase 6:

- [ ] Export ist deterministisch gegen Referenzdaten getestet.
- [ ] Start/Stop/Fehlerpfade funktionieren mit Fake-Treiber.
- [ ] Manuelle Hardwaretests sind separat protokolliert; sie blockieren keine
      hardwarelosen Testläufe.
- [ ] `native/src/laser.rs` enthält keine konkurrierende Service-Logik mehr.

## Phase 7 — Native-Struktur und Bedienqualität

Ziel: Spike-Struktur in wartbare Produktstruktur überführen.

- [ ] `native/src/app.rs` nach Verantwortung zerlegen, erst nachdem die
      Application-Grenze stabil ist:
  - [ ] Event-/Inputübersetzung;
  - [ ] Tool-/Gestenzustand;
  - [ ] Renderer und Cache-Invalidierung;
  - [ ] UI-Komposition;
  - [ ] Dialog-Präsentationszustand.
- [ ] UI-Größen, DPI-Skalierung und Ultrawide-/kleine Fenster testen.
- [ ] Tooltips, deaktivierte Zustände, Fokus und Tastaturnavigation.
- [ ] Rechte Panels sinnvoll skalierbar/resizable machen.
- [ ] Leere, Lade-, Fehler- und Fortschrittszustände gestalten.
- [ ] Ungespeichert-/Projektstatus deutlich sichtbar machen.
- [ ] Performancebudgets dokumentieren und messen:
  - [ ] Startzeit;
  - [ ] große SVG öffnen;
  - [ ] Pan/Zoom/Drag;
  - [ ] Speicherverbrauch;
  - [ ] Cache-Neuaufbau nur bei relevanter Zustandsänderung.

Abnahme Phase 7:

- [ ] Keine einzelne Native-Datei wird zum neuen fachlichen Sammelmodul.
- [ ] UI bleibt bei Referenzprojekt reaktionsschnell.
- [ ] Wesentliche Workflows sind ohne versteckte Tastenkürzel auffindbar.

## Phase 8 — Tauri/Svelte endgültig entfernen

Voraussetzung: Phasen 0–7 und Gesamt-Definition-of-Done sind erfüllt oder jede
bewusste Ausnahme ist hier dokumentiert.

- [ ] Letzte Funktionsmatrix prüfen: keine ungeklärten Commands.
- [ ] Noch relevante Tests aus `frontend/src-tauri` nach Core/Application
      verschieben.
- [ ] `luxifer/frontend/src-tauri/` löschen.
- [ ] `luxifer/frontend/src/` und WebGL-/Canvas-Helfer löschen.
- [ ] Node-/Svelte-/Vite-/Tauri-Konfiguration und Lockfiles löschen, sofern sie
      keinem anderen Workspace-Teil dienen.
- [ ] Tauri-spezifische Buildskripte, `dev.sh`-Workarounds und CI-Schritte
      entfernen.
- [ ] IPC-/JavaScript-DTOs entfernen; Rust-Domänentypen nicht künstlich an alte
      Serialisierungsformen binden.
- [ ] Workspace-Kommentare und `exclude` für `frontend/src-tauri` bereinigen.
- [ ] tote Dependencies und Feature-Flags entfernen (`cargo machete` nur falls
      bereits verfügbar; sonst manuell und per Build prüfen).
- [ ] README, Entwicklerdokumentation, Roadmap und ADR-Verweise aktualisieren.
- [ ] ADR 0008/0009 als historisch abgelöst markieren; ADR 0010/0011 bleiben
      aktive Entscheidung.

Endprüfung:

```bash
cargo fmt --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p luxifer-native --release
```

- [ ] Native Release manuell starten.
- [ ] Projekt-Roundtrip und Referenzdatei prüfen.
- [ ] Hardwareloser Laser-Smoke-Test.
- [ ] `rg -n "tauri|svelte|WebView|invoke\(" .` prüfen und verbleibende Treffer
      entweder entfernen oder als historische ADR-Inhalte bestätigen.
- [ ] Abschließenden Tauri-Abbau als eigenen Commit dokumentieren.

## Offene Architekturfragen

Diese Fragen müssen vor dem jeweils betroffenen Schnitt entschieden und hier
eingetragen werden; sie blockieren nicht Phase 0/1:

- [ ] Soll `luxifer-application` ein Crate oder zunächst ein Modul in Native
      sein? Vorgabe aus ADR 0011: eigenes Crate bevorzugt, weil es GPU-/UI-frei
      testbar und die Abhängigkeitsrichtung sichtbar bleibt.
- [ ] Synchroner oder asynchroner Geräte-/Dateiablauf? UI darf in keinem Fall
      während langer I/O- oder Treiberoperationen blockieren.
- [ ] Manueller Save oder Autosave? Kein stilles Übernehmen des bisherigen
      Verhaltens ohne festgelegten Projektworkflow.
- [ ] Welche Tauri-Funktionen werden bewusst nicht übernommen? Jede Streichung
      benötigt eine kurze Begründung in der Funktionsmatrix.
- [ ] Welche Plattformen sind Release-Ziele und welche nativen Dateidialoge/
      Pfadregeln gelten dort?

## Übergabenotiz für den nächsten Agenten

Nächster sinnvoller Schritt ist **Phase 0**, nicht weitere UI-Implementierung:

1. `git status --short` lesen und die vorhandenen Native-Änderungen schützen.
2. Die vollständige Inventur in `docs/native_function_matrix.md` gegen die
   aktuelle UI prüfen und Statusabweichungen ergänzen.
3. Native-Buttons den Funktionen zuordnen und Attrappen deaktivieren.
4. Danach den dort beschriebenen minimalen `luxifer-application`-Schnitt
   umsetzen.

Bei Unsicherheit gilt die Grenze aus ADR 0011: Core besitzt Fachregeln,
Application besitzt Abläufe und Ressourcenkoordination, Native besitzt nur
Interaktion und Darstellung.
