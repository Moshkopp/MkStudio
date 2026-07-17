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
- [x] Tauri-unabhängige Anwendungsschicht existiert (`luxifer-application`
      mit `EditorSession`, `ProjectService`, `LaserService`, `AppError`).
- [ ] Tauri/Svelte ist entfernt.

Die zu Beginn dieser Liste erwähnten uncommitteten Nutzeränderungen sind längst
als Spike-Checkpoints committet; die Arbeitskopie ist sauber. Die vom Nutzer am
laufenden Fenster gesammelte Bedienungs-/Mängelliste liegt in
[`docs/native_todo_bedienung.md`](native_todo_bedienung.md) und wird pro Schnitt
fortgeschrieben.

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

- [x] Aktuelle Native-Änderungen prüfen und als eigenen Spike-Checkpoint sichern.
- [x] Native-Demodaten aus `App::new` entfernt; ein optionales CLI-Argument
      importiert ausschließlich die ausdrücklich übergebene Nutzerdatei.
- [x] Alle sichtbaren Native-Aktionen inventarisieren (Funktionsmatrix und
      `docs/native_todo_bedienung.md`).
- [x] Nicht implementierte Aktionen deaktivieren oder klar kennzeichnen
      (Pattern Fill/Bridge melden einen stabilen `not_migrated`-`AppError`;
      Trim ist als klickbasiertes Werkzeug umgesetzt).
- [x] Fehlerhafte Aktionen entweder reparieren oder bis zu ihrem Schnitt
      deaktivieren (P1-Regressionen A1/A4/B1/C1 sind behoben; Rest siehe
      Bedienungsliste).
- [x] Tauri-Commands auf Funktionsebene vollständig inventarisieren:
  - [x] `frontend/src-tauri/src/lib.rs`
  - [x] `commands/shapes.rs`
  - [x] `commands/edit.rs`
  - [x] `commands/image.rs`
  - [x] `commands/project.rs`
  - [x] `commands/laser.rs`
- [x] Für jeden Command eine Zeile in der Funktionsmatrix ergänzen:

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
      `bewusst entfällt`; es gibt keine Attrappen. (Fast erfüllt: Pattern
      Fill/Bridge melden `not_migrated`; offen bleiben Demo-Startinhalt und
      „Aztec laden“.)
- [x] Jeder Tauri-Command ist einer Zielverantwortung zugeordnet.

## Phase 1 — `luxifer-application` als Grenze einführen

Ziel: Testbare Sitzung und konsistenter Aufrufpfad vor weiterer Migration.

- [x] Workspace-Crate `luxifer/application` anlegen und in `Cargo.toml`
      aufnehmen.
- [x] Abhängigkeiten nur in zulässiger Richtung aufbauen:
      `native -> application -> core/drivers`; niemals zurück.
- [x] Fachlich geschnittene Application-Dienste definieren
      (`EditorSession`, `ProjectService`, `AssetService`, `LaserService`).
- [x] `EditorSession` mit eindeutigem Besitz des laufenden `AppState` einführen.
- [x] Einheitliches `AppError` definieren:
  - [x] stabiler Fehlercode;
  - [x] nutzerlesbare Meldung;
  - [x] optionale technische Ursache/Quelle;
  - [x] Konvertierungen für I/O, Projektformat, Import und Treiberfehler.
- [x] Ergebnis-/Statusmodelle UI-unabhängig halten; keine `egui`, `winit`,
      `wgpu` oder Tauri-Typen.
- [x] Native besitzt eine zentrale `AppError`-Anzeige; technische Ursachen
      bleiben als Details erhalten und werden an den Startgrenzen geloggt.
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

- [x] Szene lesen und Render-Invalidierung aus Application-Zustand ableiten
      (Core-`render_rev` statt Per-Frame-Hash; Auswahl liegt im Overlay).
- [x] Auswahl: Klick, additiv, Rechteckauswahl, Auswahl löschen.
- [x] Zeichnen: Rechteck, Ellipse, Linie, Polygon, Polylinie, Spline und Bézier
      einschließlich Abbruch und Abschluss.
- [x] Transformieren: Verschieben, Skalieren, proportional Skalieren, Rotieren
      und Spiegeln laufen über `EditorSession`.
- [x] Transform-Handles und BBox ausschließlich aus kanonischer Core-Geometrie
      (`resize_to_cursor`/`keep_aspect`/`Handle::is_corner` im Core).
- [x] Layer/Farbe: Aktivieren, Sichtbarkeit, Job-Aktivierung, Sperre, Air Assist
      und Reihenfolge laufen über `EditorSession`; Parameterdialog und
      numerische Layerwerte (`set_layer_params` mit Validierung) sind migriert.
- [x] Löschen, Gruppieren, Aufheben, Undo und Redo laufen über
      `EditorSession`.
- [x] Tastaturkürzel einschließlich Fokusregeln für Textfelder/Dialoge
      (typisierte `Shortcut`-Zuordnung; Gate blockiert bei fokussiertem Feld
      oder offenem modalem Dialog).
- [x] Jede direkte Move-/Resize-/Rotate-Geste erzeugt genau einen sinnvollen
      Undo-Schritt.
- [x] Abbruch einer direkten Manipulationsgeste stellt den Ausgangszustand
      wieder her.
- [x] Native-spezifische Geometrie-Duplikate aus `app.rs` entfernt
      (`resize_target`/`keep_aspect`/`is_corner` → Core). Der Drag-Snapshot für
      das Aufschaukel-Fix bleibt bewusst native Präsentationslogik.

Abnahme Phase 2:

- [x] Automatisierte Tests für Auswahl- und Transformregeln (Core `interact`/
      `state`, Application-Session).
- [ ] Manueller Smoke-Test: zeichnen, mehrfach auswählen, bewegen, skalieren,
      rotieren, Farbe ändern, sperren, Undo/Redo, löschen. (Offen: verlangt
      interaktiven Fensterlauf; automatisierte Pfade sind grün.)
- [x] Bekannte Start- und Geometrie-Panics entfernt: GPU/Eventloop/Worker sind
      fallibel; Scanline/Nesting behandeln NaN und Infinity robust.

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
und ist ohne winit/egui testbar. Das Eingabe-Gate (`App::input_blocked`)
blockiert Canvas-Shortcuts, wenn ein Textfeld den Tastaturfokus hat
(`egui::Context::wants_keyboard_input`) ODER ein modaler Dialog offen ist
(Layer-/Text-/Laser-Dialog). `wants_keyboard_input` allein greift nur bei
fokussiertem Feld; ein bloß geöffneter Dialog ohne aktives Feld ließe sonst
Delete/Werkzeugwechsel/Undo durch und würde die Szene dahinter verändern. Die
Leertaste ruht als Pan-Modifier zwar beim Drücken, ihr Loslassen kommt aber
immer durch, damit `space_down` nicht hängen bleibt, wenn während gehaltenem
Space ein Dialog den Fokus übernimmt. Zusätzlich behoben: Undo/Redo verlangen
jetzt Strg (ein nacktes „z"/„y" war zuvor Undo/Redo). Die Ausführung
(`App::apply_shortcut`) läuft weiter über die `EditorSession`; die reine
Taste→Aktion-Zuordnung bleibt Native-Präsentation. Validierung: 268
Workspace-Tests (17 Native, davon 6 Shortcut-Tests) und Clippy mit `-D warnings`
grün; native App startet und rendert ohne Panic.

Aufräumschnitt Render/Geometrie 2026-07-12 (Phase 2 abgeschlossen): Zwei
verbliebene Aufräumpunkte erledigt.

1. Render-Invalidierung aus dem Zustand statt Per-Frame-Hash: `AppState` führt
   eine monoton steigende `render_rev`, die an derselben Stelle wie die
   Bounds-Invalidierung steigt (jede Geometrie-/Struktur-Mutation, undo/redo).
   Native vergleicht nur noch diese `u64` statt jeden Frame alle Shapes/Layer zu
   hashen (`scene_fingerprint` entfällt). Die Auswahl-Akzentuierung wurde dazu
   aus dem gecachten Vertex-Puffer ins Overlay gezogen (`selected_outlines`),
   sodass der Cache rein an der Geometrie hängt; Auswahländerungen ohne Mutation
   bauen ihn nicht mehr neu. `project_open` erzwingt den Neuaufbau, weil der
   geladene State einen eigenen Zähler mitbringt. Nebenbei ein latenter
   Bestandsfehler behoben: der Bild-Textur-Sync hing an einer stets falschen
   Bedingung (`fp != last_fp` nach der Zuweisung) und lief faktisch nur über
   `image_dirty`.
2. Geometrie-Duplikate in den Core: `resize_to_cursor`, `keep_aspect` und
   `Handle::is_corner` leben jetzt in `luxifer-core` (`interact.rs`) mit ihren
   Tests; Native ruft nur noch auf. Der native Drag-Snapshot (Aufschaukel-Fix)
   bleibt Präsentationslogik.

Validierung: 270 Workspace-Tests (207 Core inkl. Revisions- und
Geometrie-Tests, 12 Native) und Clippy mit `-D warnings` grün; native App
startet und rendert ohne Panic.

## Phase 3 — Projekt, Versionen und Assets

Ziel: Verlustfreies Arbeiten und vollständiger Datei-/Asset-Lebenszyklus.

- [x] Tauri-Projektcommands und `native/src/project.rs` gegeneinander geprüft.
- [x] Eine kanonische `ProjectService`-Implementierung in Application
      hergestellt; `native/src/project.rs` gelöscht.
- [x] Neues Projekt, Liste, Öffnen, Speichern und „Neue Version“.
- [x] Umbenennen, Löschen, Export von Projekten (Dienst + Browser-UI mit
      Umbenennen-Entwurf und zweistufigem Löschen).
- [x] Version öffnen/löschen (Dienst + Versionsliste im Browser-Detailbereich;
      Dirty-Guard beim Ersetzen des Canvas; PNG-Thumbnails noch offen).
- [x] Asset-Verzeichnis und `asset_id`-Referenzen unverändert (der Dienst nutzt
      die kanonische Core-API `ProjectFile`; keine Base64-Dauerablage).
- [x] `AssetService` kapselt Katalog, Datei-/Fontimport, Vorbereitung,
      Thumbnails, Tags und Löschen/Ausblenden; Native-Worker koordinieren nur
      Hintergrundausführung und UI-Rückgabe.
- [x] Manuell speichern beibehalten (kein Autosave; ADR 0003, Strg+S-Workflow).
- [x] Dirty-Guard bei Neu, Öffnen und Programmende (`request_close`); Schließen
      ohne Beenden = Projektwechsel, deckt Neu/Öffnen bereits ab.
- [ ] Atomisches Speichern beziehungsweise sichere Fehlerbehandlung bei
      Teilfehlern prüfen.

Abnahme Phase 3:

- [x] Roundtrip-Test (anlegen/speichern/öffnen mit Vektor-Shape; Text-/Bild-
      Assets über die Core-`ProjectFile`-Kette bereits in den Native-Tests
      abgedeckt).
- [x] Versionswechsel verliert keine Assets/Metadaten (Dienst nutzt die
      kanonische Core-Versions-API; `version_anlegen_und_auflisten`-Test).
- [x] Schreibfehler lässt den bisherigen Stand verwendbar (offenes Projekt
      bleibt bei Öffnen-Fehler erhalten; Test
      `oeffnen_unbekannt_laesst_bisheriges_projekt_erhalten`).
- [x] `native/src/project.rs` gelöscht — keine konkurrierende Fachlogik mehr.

Projektschnitt 2026-07-12 (Phase 3 im Kern abgeschlossen): `ProjectService`
in luxifer-application ersetzt das native `ProjectBackend`; Fehler über
`AppError` (neuer `AppError::wrap`); Dirty-Guard bei Neu/Öffnen/Programmende
(`request_close`), manuelles Speichern mit `mark_saved`. UI-Aktionen
Öffnen/Export/Löschen im Browser. Validierung: 35 Application-Tests (7 Projekt-,
1 Dirty-Guard-Test) und Clippy mit `-D warnings` grün.

Projektbrowser-Schnitt 2026-07-12 (E4): Master-Detail-Browser mit Auswahl-
Liste, Detailbereich (`ProjectService::detail`, neu), live gezeichneter
Vektor-Miniatur (`ProjectService::peek_state`, neu; nutzt dieselbe
`world_outline`-Ableitung wie der Canvas), Umbenennen, Export, zweistufigem
Löschen und Versionsliste (Laden/Löschen). Dabei Service-Bug behoben:
`delete_version` verwarf den vom Core beförderten Zustand, wenn die aktuelle
Version gelöscht wurde — jetzt gibt der Dienst `Option<AppState>` zurück und
Native ersetzt den Canvas; Version-Laden und das Löschen der aktuellen Version
laufen über den Dirty-Guard. Browserauswahl/Drafts sind reiner
Native-Präsentationszustand; der Detail-Cache verfällt über den Schlüssel
`name:modified_at` bzw. `name:rev<render_rev>` von selbst. Validierung: 293
Workspace-Tests (49 Application) und Clippy mit `-D warnings` grün;
Release-Smoke-Test ohne Panic.

Offen (spätere Feinarbeit, blockiert Phase 4 nicht): atomisches Speichern bei
Teilfehlern; PNG-Thumbnails pro Version (Core-Speicherpfad vorhanden, Dienst
übergibt leeres PNG).

Preview-Schnitt 2026-07-12 (D2 abgeschlossen): `EditorSession::job_preview`
und `LaserService::plan` planen jetzt beide MIT Asset-Auflösung über den
gemeinsamen Resolver `application::assets::resolve_luma` — die Vorschau zeigt
exakt das, was der Job tut. Dabei wurde eine gefährliche Lücke geschlossen:
Der echte Job/Export plante zuvor ohne Assets und hätte Bild-Layer
stillschweigend übersprungen. Nativ zeichnet der Preview-Reiter die
verarbeiteten Rastertexturen (Pixel 255 = gebrannt → Rasterfarbe, sonst
transparent) statt der Design-Texturen, plus eine Legende (Farben je
Segmentart, Arbeitsweg/Leerfahrt/Job-Fläche), die beim Preview-Vertex-Aufbau
nebenbei entsteht (kein zweiter Preview-Lauf). `import_path` importiert nun
auch Bilddateien (Vorarbeit F1). Validierung: 296 Workspace-Tests (53
Application, davon 3 neue Raster-Tests), Clippy `-D warnings`, Release-
Smoke-Test mit Screenshot (Legende + Rasterbild sichtbar, 430 fps).

## Phase 4 — Import, Text und Bildbearbeitung

Ziel: Vollständige Erzeugungs- und Bearbeitungsworkflows statt Import-Demos.

- [x] Nativer Dateidialog ist nur Pfadlieferant (rfd; Abbruch mutiert nichts).
- [x] SVG-/DXF-Import mit Fehlerbehandlung (`AssetService::import_path`).
- [x] Bildimport mit Asset-Anlage und Textur-Invalidierung (`image_dirty`).
- [x] Bildparameter: Modus, Schwelle, Helligkeit, Kontrast, Gamma und Invert
      (`EditorSession::set_image_params`, validiert; Dialog per Doppelklick).
- [x] Live-Bildvorschau/Dithering im Dialog über die kanonische Core-Pipeline;
      Parameteränderungen aktualisieren eine gecachte Native-Textur, ohne den
      Editorzustand vor „Speichern“ zu mutieren.
- [x] Systemfonts auflisten, Text anlegen und Text editieren (Doppelklick →
      `replace_text_block`).
- [x] Fehlende/ungültige Fonts und nicht unterstützte Dateien werden als
      stabile `AppError`s behandelt und durch Application getestet.
- [x] Trace-Workflow (Bild → Vektor): `EditorSession::trace_image` mit
      LUT-Vorverarbeitung; UI im Bild-Dialog (Schwelle/Invert); Fehlerpfade
      getestet. Bild-Zuschneiden läuft in einem eigenen Dialog mit Live-Vorschau.

Abnahme Phase 4:

- [ ] Referenz-SVG/DXF/Bild/Text lassen sich importieren, speichern, erneut
      öffnen und bearbeiten.
- [ ] Abbruch im Dateidialog verändert das Projekt nicht.
- [ ] Fehler erzeugen keine leeren Layer, verwaisten Assets oder Undo-Leichen.

## Phase 5 — Geometrie- und Arrange-Werkzeuge

Ziel: Alle produktiv benötigten Operationen mit expliziten Voraussetzungen.

- [x] Ausrichten und Verteilen inklusive Gruppen über Application/Core.
- [x] Gruppieren/Aufheben und Spiegeln über Application/Core.
- [x] Boolean: Vereinigung, Schnitt und Differenz (Parameterdialog über Session).
- [x] Offset und Fillet (Distanz-/Radiusdialog über Session).
- [x] Bridge/Haltesteg als eigene Canvas-Geste mit Breitenentwurf, Endpunkt-
      Nachbearbeitung, Application-Fehlern und Undo; Ecken-Fillet läuft über
      den vorhandenen Fillet-Dialog/Core.
- [x] Trim entfernt den Core-berechneten Abschnitt zwischen den nächsten
  Schnittpunkten; Native zeigt denselben Abschnitt beim Hover, jeder Klick ist
  ein Undo-Schritt (ADR 0016).
- [x] Nesting und Nest-Fill (über Session; feste 2 mm — Gap-Dialog optional).
- [x] Pattern Fill (Parameterdialog über Session, validiert; leere Treffer
      melden einen Fehler statt stiller No-Op) und Coaster-Einfügen (über
      Session vorhanden).
- [ ] Bézier/Spline: Anlegen (vorhanden), Segment-Hit-Test, Knoten teilen/
      löschen, glatt/eckig und Handles ziehen (Node-Editing offen).
- [ ] Aktionen bei ungeeigneter Auswahl deaktivieren; Grund per Tooltip oder
      Statusmeldung erklären (Session liefert bereits stabile AppError-Gründe;
      UI-Deaktivierung/Tooltip offen).

Abnahme Phase 5:

- [ ] Ergebnis- und Regressionstests liegen überwiegend im Core.
- [ ] Native prüft nicht selbst geometrische Voraussetzungen nach.
- [ ] Jede mutierende Operation ist Undo/Redo-fähig.

## Phase 6 — Vorschau, Job und Laser

Ziel: Sicherer durchgängiger Weg vom Design zur Maschine.

- [x] Jobparameter (Startmodus/Anker) aus dem Dienst; Jobvorschau siehe unten.
- [x] Native GPU-Vorschau für Cut/Fill/Raster/Image implementieren
      (Cut/Fill/Travel als Linien, Bilder als verarbeitete Rastertextur,
      Legende mit Kennzahlen; D2-Schnitt).
- [ ] Vorschau-Simulation und Monitorzustand festlegen und umsetzen
      (bewusst niedrige Priorität).
- [x] Tauri-Lasercommands und `native/src/laser.rs` inventarisiert.
- [x] Ein kanonischer `LaserService` in Application:
  - [x] Registry laden/speichern;
  - [x] Profile anlegen/bearbeiten/löschen/aktivieren;
  - [x] verfügbare Aktionen abfragen;
  - [ ] Ping/Verbindung/Position (offen; nicht im Backend enthalten);
  - [x] Start/…/Export laufen über `run_action`/`export_to`;
  - [x] Jog und Home;
  - [x] Fehler als stabiler `AppError`; Verbindungsabbruch über Treiberfehler.
- [x] UI erzeugt keinen Treiber selbst — nur der `LaserService` (driver_for).
- [ ] Gefährliche Aktionen benötigen klare Zustände, Sperren und Rückmeldung.
- [x] Hardwarelose Tests mit Fake-Ruida (Aktionen, Export, kein aktiver Laser).

Abnahme Phase 6:

- [x] Export erzeugt mit Fake-Ruida nicht-leere Bytes (Test).
- [x] Fehlerpfad ohne aktiven Laser liefert stabilen `AppError` (Test).
      Start/Stop gegen echte HW bleiben manuelle Hardwaretests.
- [ ] Manuelle Hardwaretests separat protokollieren: Ruida kann aktuell geprüft
      werden; GRBL bleibt mangels Hardware zurückgestellt.
- [x] `native/src/laser.rs` gelöscht — keine konkurrierende Service-Logik mehr.

Service-Schnitte 2026-07-12 (Phasen 3/4/5/6 im Kern): Die zwei fehlplatzierten
Native-Backends sind als Application-Dienste gekapselt — `ProjectService` und
`LaserService` (beide mit `AppError`, hardwarelos/roundtrip-getestet);
`native/src/{project,laser}.rs` sind gelöscht. Bildparameter
(`set_image_params`) und die Geometrie-Parameterdialoge (Boolean/Offset/Fillet)
laufen über die Session; Text-Editieren und Bildbearbeitung per Canvas-
Doppelklick. Dirty-Guard schützt Neu/Öffnen/Beenden. 42 Application-Tests grün.
Offen als bewusste Feinarbeit (blockiert Phase 8 nicht): Job-/GPU-Vorschau
(Cut/Fill/Raster/Image), Trace, Pattern-Fill, Bézier-Node-Editing, Laser-Ping/
Position, UI-Deaktivierung ungeeigneter Aktionen, Projekt-Umbenennen-Dialog/
Versionsliste. Die Live-Bildvorschau ist inzwischen umgesetzt.

## Phase 7 — Native-Struktur und Bedienqualität

Ziel: Spike-Struktur in wartbare Produktstruktur überführen.

- [x] `native/src/app.rs` nach Verantwortung zerlegt (app.rs ~1481 → ~907;
      Details siehe canvas-/render-Zerlegungsnotiz unten):
  - [x] Event-/Inputübersetzung (`canvas/input.rs`);
  - [x] Tool-/Gestenzustand (`canvas/state.rs` + `canvas/gestures.rs`);
  - [x] Renderer und Cache-Invalidierung (`render/mod.rs`, `Renderer`);
  - [x] UI-Komposition (`ui.rs` → `ui/` nach Panels/Dialogen zerlegt);
  - [x] Dialog-Präsentationszustand (`ui/state.rs`).

Native-Strukturschnitt 2026-07-12 (begonnen): Der UI-Monolith `ui.rs`
(1025 Zeilen) wurde rein mechanisch nach Verantwortung in `ui/{mod,project,
tools,layers,palette,arrange}.rs` und `ui/dialogs/{layer,text,laser_settings}.rs`
zerlegt — ohne Verhaltens-/API-Änderung.

UiAction-Grenze 2026-07-12 (Fundament + Pilot): Konkretisierung der ADR-0011-
Regel „UI erzeugt Absicht, App koordiniert" für Native. Ein Panel zeichnet und
gibt `Vec<UiAction>` zurück, statt `App` zu mutieren; der Root führt sie über
`App::dispatch` aus. Umgesetzt sind das `UiAction`-Enum (`ui/action.rs`),
`App::dispatch` und die Rückgabe aus dem `arrange`-Panel über
`TopBottomPanel::show(...).inner`. Bewusst nur `arrange_bar` als Pilot (fast nur
einfache Aktionen, liest nur die Auswahlanzahl, kein Dialogentwurf/I/O), um die
Ergonomie der Grenze zu prüfen, bevor die übrigen Panels folgen.

Regel für die weitere Migration:
- Reine Aktionen → `UiAction`-Variante + `App::dispatch`.
- Lesezugriffe → das Panel erhält die nötigen Werte/View-Modelle als `&`.
- Dialog-/Textfeld-Entwürfe (immediate mode) → das Panel erhält seinen
  kurzlebigen Draft weiterhin als `&mut` (nicht `&mut App`); das Übernehmen läuft
  als `UiAction`.

Das `UiAction`-Enum wächst dabei schnittweise mit; noch nicht migrierte Panels
behalten vorübergehend `&mut App`.

UiAction-Grenze 2026-07-12 (alle Panels/Dialoge migriert): Nach dem Piloten
wurden panelweise umgestellt: `palette`/`shape_picker`, `tools`, `layers` (mit
`LayerRow`-View-Model), `project` (Textfeld-Entwurf als `&mut String`),
`topbar` (aus `build` ausgelagert), Fehler-/Statuszeile (`ui/status.rs`) sowie
alle drei Dialoge (Layer/Text/Laser). Dialoge melden über `DialogOutcome` bzw.
`LaserDialogOutcome`, ihr Entwurf wird als `&mut`-Draft gereicht, der
Draft-Lebenszyklus liegt am Root. Ergebnis: Nur noch `ui/mod.rs` (Composition
Root) kennt `App`; kein Panel-/Dialog-Modul importiert `App` mehr.

laserpanel-Schnitt 2026-07-12 (UiAction-Grenze abgeschlossen): Auch das
Laser-Bedienpanel (~420 Zeilen) ist migriert. Es bekommt eine reine `LaserView`
(Profile, aktive Id, Ampel-Slots, Export-Fähigkeit, Statusmeldung — vom Root
abgeleitet, der dafür `laser_backend.actions()` aufruft) und den bearbeitbaren
`&mut LaserUi` (Slider/Anker/Startmodus), und liefert `Vec<UiAction>`
(LaserSelect/LaserRun/LaserExport/LaserJog/LaserHome/OpenLaserSettings). Das
frühere modulinterne `PanelAction`-Enum entfällt. Damit importiert **kein**
UI-Modul mehr `App`.

Bewusst NICHT über UiAction geführt: die reine Panelbreiten-Rückschreibung
(`left_w`/`right_w`) ist Layout-Rückmeldung von egui an den Root, kein
Fachzustand — dokumentierte Ausnahme, kein offener Schuldposten.

canvas-/render-Zerlegung 2026-07-12 (abgeschlossen): Der native App-Monolith
wurde nach Verantwortung zerlegt (app.rs von ~1481 auf ~907 Zeilen):
- `canvas/scene.rs` + `canvas/overlay.rs`: der reine „Zustand → Vertices"-Aufbau
  (Basis-Puffer bzw. Frame-Overlay) als App-freie Funktionen; `OverlayInput`
  bündelt den nur gelesenen Zustand.
- `canvas/state.rs` (`CanvasState`): der Interaktions-/Kamerazustand (cam, tool,
  active_shape, drag, cursor, Modifier, poly_pts); App hält ein `canvas`-Feld.
- `canvas/gestures.rs`: die Maus-Gesten als `impl CanvasState` (+ `&mut
  EditorSession`); shape-erzeugende Gesten geben `bool` zurück, den Accent
  frischt der Root auf.
- `canvas/input.rs`: `map_keycode` und die reinen Zeiger-Events
  (`handle_pointer_event`).
- `render/mod.rs` (`Renderer`): besitzt Gpu, egui-Wgpu-Renderer/-State,
  Bild-Store, Vertex-Cache/Revision und FPS. `App::render` baut den egui-Frame
  (die `ui::build`-Closure braucht `&mut App`) und übergibt `FullOutput` + eine
  nur-lesende `FrameScene` an `Renderer::draw_frame`.

Bewusst NICHT weiter zerschnitten: Was in `app.rs` bleibt, ist echter
Composition-Root — dünne Delegatoren zur Session, `dispatch`, Dialog-
Lebenszyklen, Import-/Datei-Dialog-Verdrahtung sowie `render`/`window_event` als
Frame-/Event-Einstieg. Diese in weitere `impl App`-Blöcke über mehrere Dateien
zu schneiden würde Zeilen verschieben, aber keine Verantwortung entkoppeln
(kosmetisch). Der Laser-Aktionsblock (`laser_backend`-Koordination) ist die
einzige verbliebene eigene Verantwortung mit Fremd-Ressource; er gehört jedoch
in den Application-`LaserService` (Phase 6), nicht in einen kosmetischen
Native-Split — daher hier belassen. `left_w`/`right_w` bleiben Layout-Rück-
schreibung wie dokumentiert.

Größenstand 2026-07-12 (ehrlich): `app.rs` ist durch die seither ergänzten
Dialog-Lebenszyklen (Text/Layer/Bild/Geo-Op/Dirty-Guard), `dispatch` und die
Projekt-/Laser-Koordination wieder auf ~1190 Zeilen gewachsen. Das ist
weiterhin Composition-Root-Arbeit, kein neues Fachmodul — aber die Regel
bleibt: neue Fachlogik gehört in Core/Application, neue Interaktion in
`canvas/`, neue Panels/Dialoge in `ui/`. Ein erneuter Schnitt lohnt erst,
wenn ein Block eigenen Zustand mit schmaler Schnittstelle bildet (Kandidat:
Dialog-Lebenszyklen als eigenes Modul mit `DialogHost`-artiger API).

- [ ] UI-Größen, DPI-Skalierung und Ultrawide-/kleine Fenster testen.
- [ ] Tooltips, deaktivierte Zustände, Fokus und Tastaturnavigation.
- [x] Rechte Panels sinnvoll skalierbar/resizable machen (Inspector 340 px
      vorbelegt, 300–460 px verstellbar; Bedienungsliste E1).
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
- [x] Workspace-Kommentare und den überholten `frontend/src-tauri`-Exclude
      bereinigen.
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

Stand 2026-07-16: Phasen 0–2 sind im Kern abgeschlossen (Rest: manueller
Smoke-Test), Phasen 3–6 sind im Kern umgesetzt (`ProjectService`,
`AssetService`, `LaserService`, Bild-/Text-/Geometrie-Workflows über die Session; die nativen
Duplikate `native/src/{project,laser}.rs` sind gelöscht), Phase 7 ist begonnen
(UiAction-Grenze vollständig, Canvas-/Render-Zerlegung abgeschlossen). GPU-,
Eventloop- und Worker-Initialisierung sind fallibel; Scanline/Nesting sind
gegen nicht-finite Werte gehärtet.

Ausdrücklich **offen** (nicht als fertig behandeln):

- Preview-Simulation (Scrubber/Abspielen): der Reiter selbst ist fertig
  (Cut/Fill/Travel, verarbeitete Rastertexturen, Legende — D2 abgeschlossen).
- [x] Bridge/Haltesteg einschließlich Canvas-Geste, Breitenentwurf und Undo.
- [x] Bild-Zuschneiden als eigener Dialog mit abgeleitetem Asset und Undo.
- Bézier-Node-Editing: Knoten löschen und glatt/eckig umschalten bleiben offen;
  Hit-Test, Ziehen und Teilen sind bereits im Core vorhanden.
- Projektbrowser: PNG-Thumbnails pro Version (Master-Detail-Browser mit
  Versionen/Umbenennen/Live-Miniatur ist seit dem E4-Schnitt umgesetzt).
- Laser Ping/Verbindungsstatus/Position.

Aktuell folgt zuerst die Ruida-Hardwareabnahme, danach Bézier-Node-Editing
und Projekt-/Versions-Thumbnails. Nur die Preview-Simulation bleibt niedrige
Priorität. Die GRBL-Hardwareabnahme folgt erst bei verfügbarem Gerät.
Arbeitsgrundlage ist
`docs/native_todo_bedienung.md`; nach jedem Schnitt diese Liste, die
Bedienungsliste und die Funktionsmatrix pflegen.

Bei Unsicherheit gilt die Grenze aus ADR 0011: Core besitzt Fachregeln,
Application besitzt Abläufe und Ressourcenkoordination, Native besitzt nur
Interaktion und Darstellung.
