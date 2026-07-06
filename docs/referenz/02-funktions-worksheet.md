# Funktions-Worksheet — wie welche Funktion arbeitet

Bau-Vorlage für den LuxiFer-Neustart (Rust-Core + Tauri + Svelte). Jede Zeile:
**Was** die Funktion tut, **Wie** sie arbeitet, **Quelle** in ThorBurn. Reihenfolge
= sinnvolle Implementier-Reihenfolge. Neu implementieren, nicht kopieren.

Legende Ort im Neuprojekt: **[Core]** = Rust (`luxifer-core`), **[UI]** = Svelte,
**[Cmd]** = Tauri-Command (Brücke Svelte↔Core).

---

## Baustein A — Datenmodell [Core]  (Quelle: `core/state.rs`)

| Funktion / Typ | Wie sie arbeitet |
|---|---|
| `Geo` (enum) | Rect{x,y,w,h} / Ellipse{cx,cy,rx,ry} / Polyline{pts,closed} / Image{…}. Alle Maße mm. |
| `Geo::bbox()` | Achsenparallele Box. Ellipse: cx-rx.. ; Polyline: min/max über pts. |
| `Geo::hit_test(px,py,tol)` | Punkt in Box ± Toleranz. (ThorBurn vereinfacht: Box-Test; Polyline-Rand optional feiner.) |
| `Geo::translate(dx,dy)` | Neue Geo mit verschobenen Koordinaten (Polyline: jeder Punkt). |
| `Geo::scale_from_handle(handle,dx,dy)` | Resize über eins der 8 Handles; Kante(n) bewegen, Mindestgröße 1. |
| `Geo::scale_in_bbox(...)` | Shape relativ zu einer Gruppen-BBox skalieren (Mehrfachauswahl). |
| `Shape` | `{ layer_id, geo, group_id, speed/power/z_override }`. **Keine Objektfarbe.** |
| `Layer` | `{ name, color, visible/active/locked, mode(Cut/Fill/Raster), speed, power, min_power, air_assist, line_step, passes, dpi, dither, z_mm }`. |
| `Layer::new(i)` | Farbe = SWATCH_COLORS[i%14], Defaults (Cut, 100mm/s, 20%). |
| `AppState` | layers, active_layer, shapes, selected, Bett, Interaktions-Zustände, pending_color, undo/redo. |

**Reihenfolge:** Zuerst `Geo` + `Shape` + `Layer` + Tests (bbox/hit_test/translate/
scale sind rein und trivial testbar). Das ist das Fundament.

---

## Baustein B — Farbe = Layer (automatisch)  [Core]  (Quelle: `state.rs:244-330`)

| Funktion | Wie sie arbeitet |
|---|---|
| `activate_color(color)` | **Der Kern.** Selektion vorhanden → Farb-Layer suchen/anlegen, `selected.layer_id = layer`, leere Layer weg. Keine Selektion → nur `pending_color` merken (Layer entsteht beim nächsten Shape). |
| `remove_empty_layers()` | Entfernt Layer ohne Shapes, remappt alle `layer_id` + `active_layer`. Mind. 1 Layer bleibt. |
| `make_shape` (Zeichnen) | Neues Shape bekommt `layer_id`: existiert Layer der `pending_color`? → nimm ihn, sonst neu anlegen. |

**Regel: Nutzer legt NIE manuell Layer an.** Er klickt Farben. Diese Funktion ist
der Grund, warum ThorBurn sich „richtig" anfühlt. **Zuerst bauen, bevor irgendein
Farb-UI entsteht.**

---

## Baustein C — Undo/Redo [Core]  (Quelle: `state.rs:235-421`)

| Funktion | Wie |
|---|---|
| `push_undo()` | Klont (shapes, layers, active_layer, selected) auf `undo_stack`, leert redo. |
| `undo()` / `redo()` | Aktuellen Zustand auf den anderen Stack, gespeicherten zurückspielen. |
| `discard_last_undo_if_no_change()` | Snapshot verwerfen, wenn sich nichts geändert hat. |

**Snapshot-basiert, nicht Command-basiert.** Jede mutierende Aktion ruft vorher
`push_undo()`. Simpel und robust.

---

## Baustein D — Canvas-Interaktion  [UI + Cmd]  (Quelle: `web/canvas_input.js`, `qml/CanvasInput.js`)

| Aktion | Wie sie arbeitet |
|---|---|
| Zeichnen (rect/ellipse/line) | press = Startpunkt + Preview-Shape; move = Größe live; release = wenn groß genug committen (`push_undo`, neues Shape mit Layer aus pending_color). |
| Polyline/Spline | Klick-Kette; letztes Element = Gummiband-Preview; Klick nahe Start / Doppelklick schließt. Spline glättet mit `catmull_rom`. |
| Polygon | Aufziehen (Zentrum+Radius+Rotation); finale Punkte via `shapes::polygon_points`. |
| Selektieren | Hit-Test von oben nach unten, Toleranz `6/zoom` px. Shift/Strg additiv. Leer-Klick = Marquee (Rubber-Band). |
| Verschieben | Startpositionen ALLER selektierten merken, dann Delta anwenden (verlustfrei). |
| Skalieren | 8 Handles; `scale_from_handle` (einzeln) bzw. `scale_in_bbox` (Gruppe). |
| Knoten editieren | Stützpunkt einer Polyline anfassen und ziehen. |
| Zoom/Pan | Wheel = Zoom um Cursor (×1.15/0.85); Mittel-Maus / Space+Drag = Pan. |
| Messen | Zwei-Punkt-Linie mit Längen/Δ-Label. |

**Tauri-Muster:** Maus-Events in Svelte → Weltkoordinaten rechnen → Tauri-Command
in den Core (z. B. `add_shape`, `move_selection`, `activate_color`) → Core mutiert
`AppState` → Svelte holt neuen Zustand und zeichnet.

---

## Baustein E — Rendering  [UI]  (Quelle: `web/canvas_render.js`, `qml/CanvasPaint.js`)

Eine `paint()`-Funktion zeichnet die ganze Szene neu (Immediate-Mode Canvas), in
fester Reihenfolge:
1. Gitter (fein 10mm sehr blass, grob 100mm, Bett-Grenze)
2. Shapes — **Strichfarbe = Farbe des Layers** (`layer.color`). Fill/Raster-Layer:
   Fläche halbtransparent.
3. Zeichen-Vorschau (gestrichelt), Polyline-Gummiband
4. Selektionsbox (gemeinsame BBox) + 8 Handles
5. Knoten (nur Node-Werkzeug), Messlinie
6. Lineale (Tick-Intervall zoomabhängig 2/5/10/20/50/100mm)

**Svelte:** `<canvas>` + 2D-Context, `requestAnimationFrame`/on-change neu malen.
Zustand kommt vom Core.

---

## Baustein F — Formgeneratoren [Core]  (Quelle: `core/geometry/shapes.rs`)

| Funktion | Wie |
|---|---|
| `polygon_points(shape,cx,cy,r,rot)` | tri/quad/penta/hex/octa/star/sun/gear → Punkte auf Ring/Doppelring. |
| `heart_points(...)` | Parametrisches Herz, normiert + gedreht. |
| `ellipse_points(cx,cy,rx,ry,segs)` | Ellipse als Polygon. |
| `catmull_rom(pts,segs)` | Spline-Glättung. |
| `bezier_path(nodes,closed,tol)` | Bézier → abgeflachte Punkte (`flatten_cubic`). |

---

## Baustein G — Projekt speichern/laden  [Core + Cmd]  (Quelle: `core/project.rs`)

| Funktion | Wie |
|---|---|
| `ProjectFile::from_state(state,name,tags)` | AppState → JSON-Struktur (Layer + Shape-Enum), Modus als String. |
| `ProjectFile::save(path,state)` | JSON schreiben (`projekt.tlp`); Bilddateien in Projektordner kopieren. |
| `ProjectFile::load` + `into_state(dir)` | JSON → AppState; Bilder aus Ordner neu laden (Graustufen + Alpha-Maske). |
| `list_projects()` / `all_tags()` | Projektordner scannen, Name+Tags lesen. |
| `data_root()` | `THORBURN_DATA_DIR` → XDG → HOME. (Neu: eigener App-Name.) |

**Format:** JSON, ein Ordner pro Projekt, Bilder daneben. Serde in Rust.

---

## Baustein H — Job-Compiler (Lasern)  [Core]  (Quelle: `hardware/job/ruida_compiler/`)

| Schritt | Wie |
|---|---|
| `compile_job(shapes,layers,start_mode,anchor,scan_offset)` | Einstieg. Leerer Job → leer. |
| Ruida-Layer bilden | Eindeutige `(layer,speed,power,min_power,z)` → Ruida-Layer (max 128). Overrides greifen. Nur aktive/entsperrte Layer. |
| Start/Anker | „aktuelle Position"/„Ursprung" → Anker-Offset (z.B. Mitte). |
| Bytes bauen | Preamble → Layer-Config → F-Block+BBox → Geometrie-Body → (Z-Move) → Trailer+Checksum (endet 0xD7). |

### Schnitt-Arten
| Art | Funktion | Wie |
|---|---|---|
| Cut | `cut::shape_points_um`, `cut_geometry` | Kontur in µm abfahren. |
| Fill | `scanline::shape_fill_segments_um`, `fill_preview_segments_mm` | Fläche mit parallelen Linien (Abstand `line_step_mm`). |
| Raster | `raster::image_geometry`, `render_job_bitmap`, `merge_gaps_in_bitmap`, `scale_gray` + `dither.rs` | Bild zeilenweise, Dithering (Floyd/Jarvis/…). |

---

## Baustein I — Protokoll & Transport (Hardware)  [Core]  (Quelle: `hardware/protocol.rs`, `transport.rs`)

| Funktion | Wie |
|---|---|
| `encode_coord(um)` / `encode_power` / `encode_speed` | Ruida-Werte (µm, 7-bit). |
| `cmd_cut_abs`, `cmd_move_abs`, `cmd_set_speed`, `cmd_move_z_rel`, `cmd_stop`, … | Einzel-Befehle als Bytes. |
| `swizzle`/`unswizzle` (magic) | Ruida-Byte-Verschlüsselung. |
| `build_packet(payload,magic)` | Checksumme anhängen. |
| `Transport::connect(ip)` / `send` / `query` | **UDP** an die Maschine, ACK/NAK-Handshake, Chunking. |

---

## Baustein J — Import & Extras [Core]  (Quelle: `core/import.rs`, `geometry/*`, `nesting*`)

| Funktion | Wie |
|---|---|
| `import_file(...)` → (Layers, Shapes) | SVG/DXF/Bild. **Farbe → Layer-Gruppierung** (`by_color`). |
| `boolean.rs` | Vereinigung/Differenz/Schnitt von Polygonen. |
| `offset.rs` / `fillet.rs` | Kontur versetzen / Ecken runden. |
| `text.rs` | Text → Polygone via Font. |
| `nesting.rs` / `nesting_poly.rs` | Teile platzsparend anordnen. |
| `image_adjust.rs` | Helligkeit/Kontrast/Gamma/Schärfen für Raster. |

---

## Empfohlene Bau-Reihenfolge (Meilensteine)

1. **M1 Fundament:** Rust-Core `Geo/Shape/Layer/AppState` + Tests (Baustein A, C).
2. **M2 Farbe=Layer:** `activate_color` + `remove_empty_layers` + Tests (B).
3. **M3 Tauri-Gerüst:** Svelte-Fenster, `<canvas>`, Tauri-Commands, Zustand holen.
4. **M4 Zeichnen+Selektion+Rendering:** rect/ellipse/line/polyline, Hit-Test, Move,
   Handles, Farbpalette (D, E, F).
5. **M5 Projekt:** speichern/laden JSON (G).
6. **M6 Laser:** Job-Compiler Cut zuerst, dann Fill/Raster; Protokoll+UDP (H, I).
7. **M7 Extras:** Import, Boolean, Text, Nesting (J).

Charon (Rust-Server) teilt sich den Core und übernimmt Sync/Koordination —
niemals Voraussetzung für lokale Arbeit.
