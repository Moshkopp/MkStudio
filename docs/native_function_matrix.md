# Native-only-Funktionsmatrix

Stand: 2026-07-13
Gehört zu: [Native-only-Migration](native_only_migration_tasks.md)

## Legende

- **Core**: Fachoperation existiert bereits im `luxifer-core`; Tauri ist im
  Wesentlichen Adapter und wird gelöscht.
- **Application**: Ablauf, I/O oder Ressourcenkoordination muss aus Tauri und/oder
  Native in `luxifer-application` extrahiert werden.
- **Native**: Betriebssystem- oder Präsentationsaufgabe bleibt im Native-Crate.
- **Prüfen**: vorhandene Native-Verdrahtung ist ein Spike und noch nicht als
  vollständig oder korrekt abgenommen.
- **Fehlt**: keine vollständige Native-Oberfläche vorhanden.

Diese Matrix beschreibt das Ziel, nicht den bloßen aktuellen Button-Bestand.
Eine Zeile wird erst `erledigt`, wenn Erfolgs- und Fehlerpfad getestet, Native
angebunden und die ersetzte Tauri-Implementierung entfernt ist.

Application-Struktur: `EditorSession` ist nach `selection`, `drawing`,
`actions` und `layers` geschnitten. Der Session-Root darf nicht erneut zum
Sammelmodul werden; Projekt-, Asset- und Laserabläufe erhalten eigene Services.

## Shell, Zustand und Einstellungen

| Tauri-Command | Ziel | Native-Stand | Migration/Abnahme |
|---|---|---|---|
| `get_scene` | Core/Application | `EditorSession`, Übergangszugriff | Session besitzt `AppState`; read-only Renderer-View später verengen |
| `swatch_colors` | Core | vorhanden | direkt aus Core, kein UI-Duplikat |
| `app_version` | Application | fehlt | Cargo-Paketversion ohne Tauri liefern |
| `job_preview` | Core/Application | nativ vollständig | Cut/Fill/Travel + verarbeitete Bild-Rasterungen + Legende; gleicher Asset-Resolver wie der echte Job; **offen:** Simulation/Scrubber |
| `get_ui_settings` | Core/Native | nativ vollständig | plattformneutral laden, Defaults bei fehlender/alter Datei |
| `save_ui_settings` | Core/Native | nativ vollständig | sanitizen, fehlersicher speichern und Theme/Raster/Dialogdarstellung anwenden |
| `undo` | Core/Application | über `EditorSession` | Strg+Z; Gesten erzeugen genau einen Undo-Schritt; Shortcut-Zuordnung getestet |
| `redo` | Core/Application | über `EditorSession` | Strg+Shift+Z und Strg+Y; Modal-/Fokus-Gate wirksam |
| `frontend_ready` | entfällt | entfällt | reiner Tauri/WebView-Lebenszyklus |

Charon-Meilenstein 1 (ADR 0012): lokaler Server auf `127.0.0.1:3737`,
`/health`, versionierter Handshake, UI-unabhängiger Application-Client sowie
persistente Aktivierung/URL und Verbindungstest in den globalen Einstellungen.
Projekt-Outbox/Inbox, Push-Verteilung, arbeitsplatzbezogene Settings-Sicherung
und Ruida-Lease-Vergabe sind noch ausdrücklich nicht enthalten. Lokales
Speichern bleibt unabhängig; Charon editiert oder merged keine Projektinhalte.

## Editor, Auswahl und Layer

Quelle: `frontend/src-tauri/src/commands/edit.rs`.

| Tauri-Command | Ziel | Native-Stand | Migration/Abnahme |
|---|---|---|---|
| `activate_color` | Core/Application | über `EditorSession` | Core verwaltet Umlayern/Pending-Farbe und leere Layer |
| `select_at` | Core/Application | über `EditorSession` | additiv, leerer Klick, Gruppen und Kameratoleranz angebunden |
| `select_rect` | Core/Application | über `EditorSession` | beide Ziehrichtungen, Gruppen und rotierte BBox über Core |
| `group_op` | Core/Application | über `EditorSession` | Voraussetzung in Application; genau ein Core-Undo |
| `ungroup_op` | Core/Application | über `EditorSession` | Application/Core; kein zusätzlicher Native-Undo |
| `move_selected` | Core/Application | Session-Geste | genau ein Undo, Cancel stellt Ausgangszustand her |
| `scale_selected` | Core/Application | Session-Geste | Lebenszyklus/Undo migriert; Anker/Flip weiter prüfen |
| `rotate_selected` | Core/Application | Session-Geste | Lebenszyklus/Undo migriert; Pivot/Metadaten weiter prüfen |
| `align` | Core/Application | über `EditorSession` | Gruppen als Einheit; kein doppelter Undo-Punkt |
| `distribute` | Core/Application | über `EditorSession` | drei Einheiten werden in Application vorausgesetzt |
| `mirror` | Core/Application | über `EditorSession` | horizontal/vertikal; Core hält Metadaten synchron |
| `clear_selection` | Core/Application | über `EditorSession` | keine Dirty-/Undo-Änderung; Escape ohne aktive Geste |
| `delete_selected` | Core/Application | über `EditorSession` | Fehler ohne Auswahl sowie Löschen/Undo/Redo getestet |
| `set_layer_params` | Core/Application | über `EditorSession` | nativer Dialog; alle Laser-/Rasterparameter validiert, Bild-Invariante, ein Undo |
| `toggle_layer` | Core/Application | über `EditorSession` | visible/enabled/locked/air_assist atomar und undo-fähig |
| `move_layer` | Core/Application | über `EditorSession` | Core remappt Shape-Layer-IDs; Application validiert Indizes |

## Formen, Text, Bézier und Geometrieoperationen

Quelle: `frontend/src-tauri/src/commands/shapes.rs`.

| Tauri-Command | Ziel | Native-Stand | Migration/Abnahme |
|---|---|---|---|
| `import_vector_file` | Application/Core | über `EditorSession::import_path` | SVG/DXF mit Fehlerbehandlung; großer Import weiter beobachten |
| `pattern_fill_op` | Core/Application | über `EditorSession::pattern_fill` | Parameterdialog (Muster/Abstände/Winkel/Größe); validiert; leere Treffer melden Fehler |
| `add_spline` | Core/Application | über `EditorSession` | Abschluss/Abbruch, Fangzone am Startknoten, ein Core-Undo-Punkt |
| `upload_font` | Application | **offen** | Zielverzeichnis, Namens-/Schreibfehler (Bedienungsliste G2) |
| `list_fonts` | Application | eigene Native-Variante (`fonts.rs`) | eine kanonische Fontquelle herstellen |
| `add_text` | Core/Application | über `EditorSession::add_text_block` | nativer Dialog; Font-Lesefehler/leere Konturen werden gemeldet |
| `text_preview` | Core/Application | **offen** | Vorschau ohne Mutation (Bedienungsliste G1) |
| `update_text` | Core/Application | über `replace_text_block` | Doppelklick auf Textblock öffnet den Dialog; atomarer Ersatz |
| `add_bezier` | Core/Application | über `EditorSession` | Drücken setzt Anker, Ziehen erzeugt Tangenten; ein Undo-Schritt |
| `add_bezier_nodes` | Core/Application | über `EditorSession` | Draft-Knoten mit `h_in`/`h_out`; geschlossener Pfad über Fangzone |
| `drag_node` | Core | **offen** | Node-Editing-Schnitt: Anker/Tangenten, smooth-Regel, Gesten-Undo |
| `split_node` | Core | **offen** | Segmentparameter und Metadaten |
| `hit_bezier_segment` | Core | **offen** | nur Core-Hit-Test, zoomabhängige Toleranz |
| `toggle_node_smooth` | Core | **offen** | tangentiale Kopplung und Undo |
| `delete_node` | Core | **offen** | Mindestknoten und Formlöschung klären |
| `trace_image` | Core/Application | über `EditorSession::trace_image` | Bild-Dialog (Schwelle/Invert); LUT wirkt vor der Schwelle; Fehlerpfade getestet |
| `boolean_op` | Core/Application | über `EditorSession::boolean` | Union/Schnitt/Differenz mit Parameterdialog (`dialogs/geo_op.rs`) |
| `offset_op` | Core/Application | über `EditorSession::offset` | Distanzdialog; Core hält harte Miter-Ecken bei konvexen Konturen |
| `bridge_op` | Core/Application | **offen** (Stub) | UI meldet `not_migrated`; Geste, Breite, ungültige Treffer |
| `fillet_corners_op` | Core/Application | **offen** | Eckenauswahl, Radiusgrenzen, Undo |
| `fillet_op` | Core/Application | über `EditorSession::fillet` | Radiusdialog über Session |
| `nest_op` | Core/Application | über `EditorSession` | Auswahlvoraussetzung und Core-Undo |
| `nest_fill_op` | Core/Application | über `EditorSession` | Auswahlvoraussetzung und Core-Undo |
| `insert_coasters` | Core/Application | über `EditorSession` | rund/eckig; genau ein Core-Undo |
| `add_rect` | Core/Application | über `EditorSession` | beide Ziehrichtungen, Mindestgröße und Undo getestet |
| `add_ellipse` | Core/Application | über `EditorSession` | normalisierte BBox, Mindestgröße und Undo getestet |
| `add_line` | Core/Application | über `EditorSession` | Mindestlänge; ungültige Geste ohne Undo |
| `add_polyline` | Core/Application | über `EditorSession` | offener Pfad, Abschluss/Abbruch und Undo |
| `shape_catalog` | Core | Native-Auswahl vorhanden | eine Core-Quelle für Katalog/Parameter |
| `add_polygon` | Core/Application | über `EditorSession` | Core-`PolyShape`, Mindestradius und Undo |

## Bilder und Assets

Quelle: `frontend/src-tauri/src/commands/image.rs` sowie Projekt-Assets.

| Tauri-Command | Ziel | Native-Stand | Migration/Abnahme |
|---|---|---|---|
| `import_image_file` | Application/Core | über Session/Asset-Store | Asset-Anlage und Textur-Invalidierung (`image_dirty`); auch per `import_path` (CLI/Schnellknopf, Vorarbeit F1) |
| `image_render` | Core/Application | teilweise Renderer | **offen:** Live-Vorschau im Dialog; Wirkung erst nach Übernahme (C2) |
| `set_image_params` | Core/Application | über `EditorSession::set_image_params` | Modus/Schwelle/Helligkeit/Kontrast/Gamma/Invert validiert; Dialog per Doppelklick |
| `project_assets` | Application/Core | über Core-`ProjectFile` | Assets laufen durch die kanonische Projektkette; keine Base64-Dauerablage |

## Projekte und Versionen

Quelle: `frontend/src-tauri/src/commands/project.rs`. Das frühere
Native-Duplikat `native/src/project.rs` ist gelöscht; kanonisch ist der
`ProjectService` in `luxifer-application` (nutzt die Core-`ProjectFile`-Kette).

| Tauri-Command | Ziel | Native-Stand | Migration/Abnahme |
|---|---|---|---|
| `new_project` | Application | `ProjectService::new_project` | Dirty-Guard vor Neu/Öffnen/Beenden; Roundtrip getestet |
| `save_project` | Application/Core | `ProjectService::save` | in-place (Strg+S-Workflow); **offen:** Thumbnail, atomarer Teilfehlerpfad |
| `save_version` | Application/Core | `ProjectService::save_version` | **offen:** Notiz und Thumbnail (Dienst übergibt leeres PNG) |
| `open_project` | Application/Core | `ProjectService::open` | Öffnen-Fehler lässt bisheriges Projekt erhalten (Test) |
| `open_version` | Application/Core | Dienst + Versionsliste im Browser | ersetzt den Canvas über den Dirty-Guard; „Laden" nur beim offenen Projekt |
| `delete_version` | Application/Core | Dienst + Browser (zweistufig) | letzte Version schützt der Core; Löschen der aktuellen ersetzt den Canvas durch die beförderte Version (Dirty-Guard) |
| `project_list` | Application/Core | `ProjectService::list` | Metadaten/Sortierung; beschädigte Projekte weiter prüfen |
| `project_detail` | Application/Core | `ProjectService::detail` + Browser | Metadaten/Versionen ohne Wechsel des offenen Projekts; Cache verfällt über `modified_at`/`render_rev` |
| `project_assets` | Application/Core | über Core-`ProjectFile` | siehe Bilder/Assets |
| `project_thumb` | Application/Core | Live-Vektor-Miniatur statt PNG | Browser zeichnet aus `peek_state`; gespeicherte PNG-Thumbnails **offen** |
| `version_thumb` | Application/Core | **offen** | PNG pro Version; Core-Speicherpfad vorhanden, Dienst übergibt leeres PNG |
| `project_delete` | Application/Core | `ProjectService::delete` + Browser-UI | zweistufige Bestätigung; offenes Projekt und I/O-Fehler behandelt |
| `project_rename` | Application/Core | Dienst + Umbenennen-Entwurf im Browser | Kollision/offenes Projekt im Dienst; Auswahl folgt dem neuen Namen |
| `project_export` | Application/Core | `ProjectService::export` + Browser-UI | nativer Zieldialog; vollständige Assets/Versionen |

## Laser, Job und Geräte

Quelle: `frontend/src-tauri/src/commands/laser.rs`. Das frühere Native-Duplikat
`native/src/laser.rs` ist gelöscht; kanonisch ist der `LaserService` in
`luxifer-application` (hardwarelos mit Fake-Ruida getestet).

| Tauri-Command | Ziel | Native-Stand | Migration/Abnahme |
|---|---|---|---|
| `laser_job_start` | Application/Core | `LaserService::run_action` | plant MIT Asset-Auflösung (Bild-Layer werden gerastert, Test); Startmodus/Anker aus dem Dienst; echte HW = manueller Test |
| `laser_list` | Application | `LaserService::load` | Registry laden; beschädigte Datei fällt auf Defaults zurück |
| `laser_save` | Application | `LaserService::save_profile` | validieren, persistieren, ID-Regel |
| `laser_delete` | Application | `LaserService::delete_profile` | aktives Profil und Persistenzfehler |
| `laser_set_active` | Application | `LaserService::set_active` | Existenz und Persistenz |
| `laser_actions` | Application/Driver | `LaserService::actions` | Capabilities des aktiven Treibers; Panel baut daraus die Slots |
| `laser_run_action` | Application/Driver | `LaserService::run_action` | verbindet vorher (`laser_connect`-Fehler mit Ziel/Ursache); kein Treiberbau in der UI |
| `laser_export` | Application/Driver | `LaserService::export_to` | nativer Zieldialog; nicht-leere Bytes mit Fake-Ruida getestet |
| `laser_jog` | Application/Driver | `LaserService::jog` | verbindet vorher; Fehlerstatus über Treiberfehler |
| `laser_home` | Application/Driver | `LaserService::home` | Verbindung und Fehlerstatus |
| `laser_position` | Application/Driver | **offen** | nicht unterstützte Geräte und Fehler |
| `laser_ping` | Application/Driver | **offen** | Timeout, offline ist kein Panic |

Zusätzlich, obwohl es keine separaten Commands sind:

- `effective_shapes`: Sichtbarkeit/Aktivierung muss eine Core-/Application-Regel
  sein, nicht pro UI neu entstehen.
- `action_from_key`: erledigt — nativ läuft alles über die typisierte
  `JobAction`, keine Stringschlüssel.
- `needs_connection` und `connect_active`: erledigt — `LaserService::with_driver`
  verbindet vor verbindungsbedürftigen Aktionen (Ziel aus dem Profil); Export
  kompiliert ohne Gerät. (War zuvor fälschlich als erledigt markiert: `driver_for`
  baute nur das Objekt, verband aber nie — Bedienungsliste E6.)

## Native-spezifische sichtbare Aktionen ohne direkten Tauri-Command

| Native-Aktion | Ziel | Aktueller Status | Abnahme |
|---|---|---|---|
| Projekt/Design/Preview/Laser-Reiter | Native | vorhanden | Preview read-only; Laser-Tab sperrt Zeichnen/Löschen, Layer temporär freigebbar |
| nativer Datei-/Ordnerdialog | Native | vorhanden | Abbruch mutiert nichts; Pfad an Application (rfd) |
| `Aztec laden` | entfernen/dev-only | **noch Demo** | kein nutzerspezifischer absoluter Pfad im Produkt |
| Fit/Zoom/Pan/Kamera | Native | vorhanden, prüfen | DPI, Cursor-Zoom, Panelgrößen, großes Fenster |
| Werkzeugauswahl/Shortcuts | Native | typisierte `Shortcut`-Ebene, getestet | Fokus-/Modal-Gate wirksam; Space-Key-up kommt immer durch |
| Drag-/Marquee-/Handle-Vorschau | Native | vorhanden | Marquee als gestricheltes Overlay; nur Präsentationszustand, Commit über Session |
| FPS-/Statuszeile | Native | vorhanden | Dev-Metrik optional; Fehler/Projektstatus klar |
| Laser-Verwaltung | Native + Application | vorhanden | Master-Detail aus dem Laser-Tab: Grunddaten, Scan-Offset und Ruida-Controller; UI hält nur Draft/Livewerte |
| Text-Dialog | Native + Application | vorhanden | Anlegen + Editieren (Doppelklick); **offen:** Vorschau (G1), eigene Fonts (G2) |
| Bildparameter-Dialog | Native + Application | vorhanden | Doppelklick aufs Bild; **offen:** Live-Vorschau (C2) |
| Rechtes Panel (Inspector) | Native | resizierbar 300–460 px | Panelbreite ist Layout-Zustand des Roots, kein Fachzustand |

## Befund für den ersten Umsetzungsschnitt (historisch, umgesetzt)

Der damals empfohlene Einstieg (Anlage von `luxifer-application` mit
`EditorSession`/`AppError`, read-only Sessionzugriff statt DTO, erste
Mutationen `delete_selected`/`undo`/`redo`, Fehleranzeige, Tests) ist
vollständig umgesetzt; die Abhängigkeitsgrenze `native → application →
core/drivers` steht. Aktuelle offene Punkte stehen in der Übergabenotiz der
[Migrations-Taskliste](native_only_migration_tasks.md) und in
[`docs/native_todo_bedienung.md`](native_todo_bedienung.md).
