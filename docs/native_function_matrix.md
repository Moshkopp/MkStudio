# Native-only-Funktionsmatrix

Stand: 2026-07-12  
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
| `job_preview` | Core/Application | fehlt | Preview aus aktuellem Sessionzustand, Fehlerpfad |
| `get_ui_settings` | Application | fehlt | plattformneutral laden, Defaults bei fehlender Datei |
| `save_ui_settings` | Application | fehlt | validieren und fehlersicher speichern |
| `undo` | Core/Application | über `EditorSession` | Basisschnitt getestet; Gesten-Undo folgt in Phase 2 |
| `redo` | Core/Application | über `EditorSession` | Basisschnitt getestet; Gesten-Redo folgt in Phase 2 |
| `frontend_ready` | entfällt | entfällt | reiner Tauri/WebView-Lebenszyklus |

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
| `import_vector_file` | Application/Core | vorhanden, prüfen | SVG/DXF, Dateifehler, großer Import, Undo |
| `pattern_fill_op` | Core/Application | UI-Aktion, prüfen | Parameterdialog, Auswahlvoraussetzung, Fehler |
| `add_spline` | Core/Application | über `EditorSession` | Abschluss/Abbruch und einzelner Core-Undo-Punkt |
| `upload_font` | Application | fehlt | Zielverzeichnis, Namens-/Schreibfehler |
| `list_fonts` | Application | eigene Native-Variante | eine kanonische Fontquelle herstellen |
| `add_text` | Core/Application | vorhanden, prüfen | Font, Metadaten, Gruppierung, Undo |
| `text_preview` | Core/Application | fehlt | Vorschau ohne Mutation |
| `update_text` | Core/Application | fehlt | bestehenden Textblock atomar ersetzen |
| `add_bezier` | Core/Application | über `EditorSession` | Basis-Zeichenablauf und Undo migriert; Tangentenregeln folgen Node-Schnitt |
| `add_bezier_nodes` | Core/Application | prüfen | Handles und geschlossener Pfad |
| `drag_node` | Core | fehlt/prüfen | Anker/Tangenten, smooth-Regel, Gesten-Undo |
| `split_node` | Core | fehlt | Segmentparameter und Metadaten |
| `hit_bezier_segment` | Core | fehlt/prüfen | nur Core-Hit-Test, zoomabhängige Toleranz |
| `toggle_node_smooth` | Core | fehlt | tangentiale Kopplung und Undo |
| `delete_node` | Core | fehlt | Mindestknoten und Formlöschung klären |
| `trace_image` | Core/Application | fehlt | Asset, Parameter, Ergebnis-/Fehlerzustand |
| `boolean_op` | Core/Application | Basis über `EditorSession` | Union-UI aktiv; Varianten-/Parameterdialog folgt |
| `offset_op` | Core/Application | Basis über `EditorSession` | Defaultwert aktiv; Distanzdialog folgt |
| `bridge_op` | Core/Application | UI-Aktion, prüfen | Geste, Breite, ungültige Treffer |
| `fillet_corners_op` | Core/Application | fehlt | Eckenauswahl, Radiusgrenzen, Undo |
| `fillet_op` | Core/Application | Basis über `EditorSession` | Defaultwert aktiv; Radiusdialog folgt |
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
| `import_image_file` | Application/Core | vorhanden, prüfen | Asset atomar anlegen; Undo/Fehler ohne Waise |
| `image_render` | Core/Application | teilweise Renderer | Parameter-Vorschau ohne dauerhafte Mutation |
| `set_image_params` | Core/Application | fehlt | alle Modi/Parameter, Undo, Textur invalidieren |
| `project_assets` | Application/Core | fehlt | nur `asset_id`, Metadaten und sichere Pfade |

## Projekte und Versionen

Quelle: `frontend/src-tauri/src/commands/project.rs`. Die aktuelle
`native/src/project.rs`-Implementierung ist bis zur Gegenprüfung als Duplikat,
nicht als Zielimplementierung, zu behandeln.

| Tauri-Command | Ziel | Native-Stand | Migration/Abnahme |
|---|---|---|---|
| `new_project` | Application | vorhanden, fehleranfällig/unvollständig | Dirty-Guard, leere Session, Name/Metadaten |
| `save_project` | Application/Core | vorhanden, unvollständig | Assets, Metadaten, Thumbnail, atomarer Fehlerpfad |
| `save_version` | Application/Core | vorhanden, unvollständig | Notiz, Thumbnail, current_version |
| `open_project` | Application/Core | vorhanden, unvollständig | Dirty-Guard, Assets, Fehler ohne Zustandsverlust |
| `open_version` | Application/Core | fehlt | Version wird kanonischer Sessionzustand |
| `delete_version` | Application/Core | fehlt | aktuelle/letzte Version schützen |
| `project_list` | Application/Core | vorhanden, teilweise | Metadaten, Sortierung und beschädigte Projekte |
| `project_detail` | Application/Core | fehlt | Versionen/Metadaten vollständig |
| `project_assets` | Application/Core | fehlt | siehe Bilder/Assets |
| `project_thumb` | Application/Core | fehlt | fehlendes Thumbnail als normaler Zustand |
| `version_thumb` | Application/Core | fehlt | fehlendes Thumbnail als normaler Zustand |
| `project_delete` | Application/Core | fehlt | offenes Projekt und I/O-Fehler behandeln |
| `project_rename` | Application/Core | fehlt | Kollision, offenes Projekt und Pfade |
| `project_export` | Application/Core | fehlt | sicheres Ziel, vollständige Assets/Versionen |

## Laser, Job und Geräte

Quelle: `frontend/src-tauri/src/commands/laser.rs`. Die aktuelle
`native/src/laser.rs`-Implementierung ist bis zur Gegenprüfung als Duplikat,
nicht als Zielimplementierung, zu behandeln.

| Tauri-Command | Ziel | Native-Stand | Migration/Abnahme |
|---|---|---|---|
| `laser_job_start` | Application/Core | teilweise | Jobparameter, Startposition, Gerätefehler |
| `laser_list` | Application | vorhanden, prüfen | Registry laden, beschädigte Datei/Defaults |
| `laser_save` | Application | vorhanden, prüfen | validieren, persistieren, ID-Regel |
| `laser_delete` | Application | vorhanden, prüfen | aktives Profil und Persistenzfehler |
| `laser_set_active` | Application | vorhanden, prüfen | Existenz und Persistenz |
| `laser_actions` | Application/Driver | vorhanden, prüfen | Capabilities des aktiven Treibers |
| `laser_run_action` | Application/Driver | vorhanden, prüfen | connect, Zustandsautomat, Rückmeldung |
| `laser_export` | Application/Driver | vorhanden, prüfen | nativer Zieldialog, deterministische Bytes |
| `laser_jog` | Application/Driver | vorhanden, prüfen | Verbindung, Grenzen, Geschwindigkeit |
| `laser_home` | Application/Driver | vorhanden, prüfen | Verbindung und Fehlerstatus |
| `laser_position` | Application/Driver | fehlt | nicht unterstützte Geräte und Fehler |
| `laser_ping` | Application/Driver | fehlt/prüfen | Timeout, offline ist kein Panic |

Zusätzlich zu prüfen, obwohl es keine separaten Commands sind:

- `effective_shapes`: Sichtbarkeit/Aktivierung muss eine Core-/Application-Regel
  sein, nicht pro UI neu entstehen.
- `action_from_key`: Stringschlüssel entfallen nativ; typisierte `JobAction`
  verwenden.
- `needs_connection` und `connect_active`: gehören in den `LaserService`, nicht
  in egui-Callbacks oder einen UI-Adapter.

## Native-spezifische sichtbare Aktionen ohne direkten Tauri-Command

| Native-Aktion | Ziel | Aktueller Status | Abnahme |
|---|---|---|---|
| Projekt/Design/Laser-Reiter | Native | vorhanden | Zustand bleibt bei Reiterwechsel konsistent |
| nativer Datei-/Ordnerdialog | Native | vorhanden | Abbruch mutiert nichts; Pfad an Application |
| `Aztec laden` | entfernen/dev-only | Demo | kein nutzerspezifischer absoluter Pfad im Produkt |
| Fit/Zoom/Pan/Kamera | Native | vorhanden, prüfen | DPI, Cursor-Zoom, Panelgrößen, großes Fenster |
| Werkzeugauswahl/Shortcuts | Native | vorhanden, prüfen | Fokusregeln und deaktivierte Werkzeuge |
| Drag-/Marquee-/Handle-Vorschau | Native | vorhanden, prüfen | nur Präsentationszustand; Commit über Application |
| FPS-/Statuszeile | Native | vorhanden | Dev-Metrik optional; Fehler/Projektstatus klar |
| Laser-Profil-Dialog | Native + Application | vorhanden, prüfen | UI validiert Darstellung, Service fachlich |
| Text-Dialog | Native + Application | vorhanden, prüfen | Vorschau, Editieren, Fehler und Abbruch |

## Befund für den ersten Umsetzungsschnitt

Der kleinste belastbare Schnitt für Phase 1 ist nicht Projekt oder Laser. Beide
enthalten bereits I/O- und Lebenszyklusfragen. Empfohlen wird:

1. `luxifer-application` mit `EditorSession` und `AppError` anlegen;
2. `get_scene` nicht als serialisiertes DTO kopieren, sondern einen direkten,
   read-only Sessionzugriff für Native definieren;
3. `delete_selected`, `undo` und `redo` als erste vollständige Mutationen über
   die Session führen;
4. Native-Fehleranzeige ergänzen;
5. Tests für Dirty-, Auswahl- und Undo-Zustand schreiben;
6. erst danach die gestenreichen Transformoperationen migrieren.

Damit wird die Abhängigkeitsgrenze mit geringem UI-Risiko bewiesen, ohne bereits
die schwierigen Projekt- oder Hardwareentscheidungen vorwegzunehmen.
