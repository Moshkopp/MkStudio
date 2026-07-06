# ThorBurn — Vollständige Analyse (belegt am Quellcode)

Analyse der Vorgänger-App **ThorBurn** als Grundlage für den LuxiFer-Neustart
(Rust-Core + Tauri + Svelte, Server = Charon). Ziel: verstehen, was die Software
**genau macht**, wenn man (a) im Canvas etwas ausführt, (b) ein Projekt
speichert, (c) den Laser bedienen will. Alle Aussagen sind mit Datei:Zeile
belegt. **Kein Code wird kopiert** — dies ist die Bauplan-Grundlage.

## 0. Aufbau des Repos

ThorBurn besteht aus **drei** Teilen, aber nur **einem** Logik-Kern:

| Teil | Sprache | Rolle |
|------|---------|-------|
| `thorburn-core` | Rust | **Die Wahrheit.** Datenmodell, Geometrie, Job-Compiler, Hardware, Import, Nesting. |
| `thorburn-qt` | QML/JS | Desktop-GUI (alt). Nutzt den Core. |
| `thorburn-server` | Rust + Web-JS | Server + Web-GUI. Nutzt denselben Core; Web-GUI ist ein Frontend über HTTP/WS. |

**Wichtig für den Neustart:** Es gibt bereits das Muster „Rust-Core + Web-
Frontend" (thorburn-server + web/*.js). Genau das bauen wir mit Tauri+Svelte
sauberer nach — Core-Logik in Rust, Svelte zeichnet nur.

---

## 1. Datenmodell (`thorburn-core/src/core/state.rs`)

Das ist das Herz. Alles dreht sich um `AppState`, `Layer`, `Shape`, `Geo`.

### 1.1 Shape — eine Form auf dem Canvas (state.rs:170-200)

```
Shape {
    layer_id: usize,          // gehört zu welchem Layer (Index in layers)
    geo: Geo,                 // die Geometrie (siehe unten)
    group_id: Option<u32>,    // gemeinsam selektieren/verschieben
    speed_override: Option<f64>,  // pro-Shape-Übersteuerung der Layer-Parameter
    power_override: Option<f64>,
    z_override: Option<f64>,      // Fokustest (relativer Z-Move)
}
```

**Ein Shape hat KEINE eigene Farbe.** Es zeigt über `layer_id` auf einen Layer;
der Layer hält Farbe UND Laserparameter. (state.rs:172)

### 1.2 Geo — die vier Geometrie-Typen (state.rs:186-200)

- `Rect { x, y, w, h }` — obere linke Ecke + Größe (mm)
- `Ellipse { cx, cy, rx, ry }` — Mittelpunkt + Halbachsen (mm)
- `Polyline { pts: Vec<(f64,f64)>, closed: bool }` — offene/geschlossene Punktfolge
- `Image { x, y, w, h, px_w, px_h, pixels: Arc<Vec<u8>>, mask, src_path, params }`
  — Graustufenbild + optionale Alpha-Maske (Crop-Ränder werden nicht gelasert)

`Geo` kann sich selbst: `bbox()`, `hit_test(px,py,tol)`, `translate(dx,dy)`,
`scale_from_handle(handle,dx,dy)`, `scale_in_bbox(...)` — reine Geometrie, alle
in state.rs:465-555. **Das ist die testbare Kernlogik, die ins neue Rust-Core
1:1 übernommen werden kann.**

### 1.3 Layer — Farbe + Laserparameter (state.rs:52-108)

```
Layer {
    name, color: [u8;3], visible, active, locked,
    mode: LayerMode,          // Cut | Fill | Raster
    speed_mm_s, power_pct, min_power_pct, air_assist,
    line_step_mm,             // Zeilenabstand für Fill (mm)
    passes,                   // Wiederholungen
    dpi, dither,              // für Raster
    z_mm: Option<f64>,        // Fokus-Z
}
```

`Layer::new(index)` vergibt reihum eine Farbe aus `SWATCH_COLORS` (14 Farben,
state.rs:3-18) und Standard-Parameter (Cut, 100 mm/s, 20% Power).

### 1.4 AppState — der gesamte Editor-Zustand (state.rs:202-233)

Enthält: `layers`, `active_layer`, `shapes`, `selected: Vec<usize>`, Bettgröße,
laufende Interaktionen (`drag_start`, `preview`, `move_drag`, `scale_drag`,
`marquee`, `poly_pts`), `pending_color`, `undo_stack`/`redo_stack`.

**Undo/Redo ist Snapshot-basiert** (state.rs:235-421): `push_undo()` klont den
kompletten Zustand (shapes+layers+active+selected) auf einen Stack. Einfach und
robust — nicht Command-basiert.

### 1.5 DAS FARB=LAYER-MODELL (state.rs:244-291) — der Knackpunkt

`activate_color(color)` ist die Kernfunktion hinter „Farbe klicken":

```
Farbe klicken:
├─ Shape(s) selektiert?
│   ├─ JA: Layer mit dieser Farbe suchen.
│   │      → existiert   → nimm ihn
│   │      → existiert nicht → NEUEN Layer mit der Farbe anlegen
│   │      Dann: alle selektierten shapes.layer_id = layer_id   ← Objekt wandert in Farb-Layer
│   │      remove_empty_layers()   ← leere Layer automatisch weg
│   └─ NEIN: nur pending_color = color merken
│          → der Layer entsteht ERST beim nächsten gezeichneten Shape
```

**Das bedeutet:** Der Nutzer legt NIE manuell einen Layer an. Er denkt in
**Farben**; das System verwaltet Layer automatisch. Farbe = Layer = Parametersatz
(wie LightBurn). `remove_empty_layers()` (state.rs:294) räumt ungenutzte Layer
auf und remappt alle `layer_id`. **Diesen Mechanismus im Neustart exakt so
übernehmen.**

---

## 2. Was passiert im Canvas? (Wirkungskette „Zeichnen/Bearbeiten")

Belegt in `thorburn-qt/qml/CanvasInput.js` (QML-GUI) und
`thorburn-server/src/web/canvas_input.js` (Web-GUI) — beide rufen denselben
Core. Ablauf beim **Zeichnen eines Rechtecks**:

1. **onPressed** (CanvasInput.js:44): Werkzeug `rect` → `newShape` mit Startpunkt
   und Größe 0.1 anlegen. (Im Rust-Core: `AppState.preview` + `drag_start`.)
2. **onPositionChanged** (CanvasInput.js:176): beim Ziehen `newShape.w/h`
   aktualisieren, `requestPaint()`.
3. **onReleased** (CanvasInput.js:242): wenn groß genug (w,h > 1mm) → Shape
   committen: **neue Farbe** = `layerColor()` bzw. `pending_color`, in die
   Shape-Liste, `save()`. Danach zurück auf Werkzeug `select`.

**Selektieren + Verschieben** (CanvasInput.js:93-124, 208-230):
- `pressSelect`: von oben nach unten (`i--`) erstes getroffenes Shape (Hit-Test
  mit Toleranz `6/viewScale` = 6 Pixel). Shift/Strg = additiv.
- Merkt sich `initialPositions` **aller** selektierten Shapes.
- `moveSelection`: verschiebt alle um das Delta zur Startposition (verlustfrei,
  kein Fehler-Akkumulieren).

**Weitere Werkzeuge:** node (Stützpunkte editieren), line, ellipse, polygon
(finale Punkte vom Backend), polyline/spline (Klick-Kette, Gummiband-Preview),
text, measure. Zoom (`onWheel`): um den Mauszeiger, Faktor 1.15/0.85.

**Rendering** (`CanvasPaint.js` / `web/canvas_render.js`): eine `paint()`-Funktion
zeichnet die ganze Szene neu: Gitter → Objekte (Farbe vom Layer) → Zeichen-
Vorschau → Selektionsbox+Handles → Lineale. Immediate-Mode.

---

## 3. Was passiert beim Speichern? (Wirkungskette „Projekt")

Belegt in `thorburn-core/src/core/project.rs`.

- **Format:** `projekt.tlp` = **JSON** (project.rs:9, 321). Pro Projekt ein Ordner
  unter `$DATA/Projekte/<name>/` (project.rs:39-46). Datenverzeichnis via
  `THORBURN_DATA_DIR` / XDG (project.rs:23-36).
- **`ProjectFile::from_state`** (project.rs:186): AppState → serialisierbare Form.
  Layer werden 1:1 gespeichert (Modus als String "cut"/"fill"/"raster"). Shapes
  als `ShapeData`-Enum (Rect/Ellipse/Polyline/Image) mit `layer`-Index und den
  Overrides.
- **Bilder:** Nur der Dateiname wird in die JSON geschrieben; die Bilddatei wird
  beim Speichern in den Projektordner **kopiert** (project.rs:311-319).
- **`into_state`** (project.rs:260): JSON → AppState zurück. Bilder werden aus dem
  Projektordner neu geladen (`load_image_shape`, project.rs:359: Graustufen +
  Alpha-Maske aus dem Bild).
- **Projektliste** (project.rs:55): scannt `Projekte/`, liest Name + Tags aus
  jeder `.tlp`.

**Fürs Neue:** JSON-Projektformat mit Layer- und Shape-Arrays, pro Projekt ein
Ordner, Bilder daneben. Serde macht das in Rust trivial.

---

## 4. Was passiert beim Lasern? (Wirkungskette „Job")

Der wichtigste und komplexeste Teil. Belegt in
`thorburn-core/src/hardware/job/ruida_compiler/mod.rs` und `protocol.rs` /
`transport.rs`.

### 4.1 Shapes → Ruida-Job (ruida_compiler/mod.rs:27)

`compile_job_z(shapes, layers, start_mode, anchor, scan_offset, final_z)`:

1. **Ruida-Layer bilden** (mod.rs:39-136): Jede eindeutige Kombination aus
   `(layer_id, speed, power, min_power, z)` wird ein **Ruida-Layer** (max. 128).
   Shape-Overrides (`speed_override` etc.) greifen hier. Nur **aktive, nicht
   gesperrte** Layer kommen rein (mod.rs:63). → Deduplizierung: viele Shapes mit
   gleichen Parametern teilen sich einen Ruida-Layer.
2. **Startpunkt/Anker** (mod.rs:150): bei „aktuelle Position"/„Benutzerursprung"
   wird ein Anker (z. B. Mitte) als Offset berechnet.
3. **Job-Bytes zusammensetzen** (mod.rs:168-200):
   `Preamble` → `Layer-Config` (pro Layer Speed/Power/Farbe) → `F-Block + BBox`
   → `Geometrie-Body` (die eigentlichen Schneid-/Fahr-Befehle) → optionaler
   `Z-Move` → `Trailer + Checksumme` (endet mit `0xD7`, mod.rs:233).

### 4.2 Die drei Schnitt-Arten (job/cut.rs, fill.rs, scanline.rs, raster.rs)

- **Cut** (`cut.rs`): Kontur abfahren — `shape_points_um` liefert die Punkte in µm.
- **Fill** (`fill.rs` + `scanline.rs`): Fläche mit parallelen Linien füllen
  (Zeilenabstand `line_step_mm`). `shape_fill_segments_um` = Scanline-Segmente.
- **Raster** (`raster.rs`): Bild zeilenweise gravieren; `render_job_bitmap`,
  `merge_gaps_in_bitmap`, Dithering (`hardware/dither.rs`: Floyd/Jarvis/Stucki/…).

### 4.3 Protokoll & Transport (protocol.rs, transport.rs)

- **Ruida-Protokoll** (`protocol.rs`): Befehle als Bytes — `cmd_cut_abs`,
  `cmd_move_abs`, `cmd_set_speed`, `encode_coord` (µm), `encode_power`. Koordinaten
  in **µm**, Werte 7-bit-kodiert (protocol.rs:38).
- **Swizzle** (protocol.rs:10-31): Ruida verschlüsselt jedes Byte (`swizzle_byte`
  mit Magic). `build_packet` hängt Checksumme an (protocol.rs:75-81).
- **Transport** (transport.rs): **UDP** an die Maschine. `connect(ip)` (Ping),
  `send(payload)` in Chunks, `query` mit ACK/NAK-Handshake (transport.rs:34-102).

### 4.4 Laser-Bedienung (Server-Seite)

`thorburn-server` bietet die Job-/Motion-Routen: Job senden
(`routes/job_send.rs`), Jog/Bewegung (`routes/motion.rs`, `web/controller/motion.js`),
Verbindung/Status (`routes/connection.rs`, `status.rs`). Die Web-GUI-Laser-Bedienung
liegt in `web/job.js`, `web/controller/*.js`.

---

## 5. Weitere Funktionen (Übersicht, Core-verortet)

| Funktion | Ort | Kurz |
|----------|-----|------|
| Import SVG/DXF/Bild | `core/import.rs` | Farbe → Layer-Gruppierung (`by_color`, import.rs:65) |
| Nesting | `core/nesting.rs`, `nesting_poly.rs` | Teile platzsparend anordnen |
| Boolesche Ops | `core/geometry/boolean.rs` | Vereinigung/Differenz von Polygonen |
| Offset/Fillet | `core/geometry/offset.rs`, `fillet.rs` | Kontur versetzen, Ecken runden |
| Text | `core/geometry/text.rs` | Text → Polygone (Font) |
| Formen | `core/geometry/shapes.rs` | Stern/Zahnrad/Herz etc. (Polygon-Punkte) |
| Bildaufbereitung | `hardware/image_adjust.rs` | Helligkeit/Kontrast/Gamma/Schärfen |
| Material-Test | `server/material_test.rs` | Test-Matrix aus Kästchen mit variierenden Parametern |

---

## 6. Kern-Erkenntnisse für LuxiFer-Neustart

1. **Rust-Core = einzige Wahrheit.** `state.rs` (Modell + Interaktions-Zustand +
   Undo) und `geometry` (bbox/hit_test/translate/scale) sind direkt übertragbar.
2. **Farbe = Layer = Parametersatz**, automatisch verwaltet (`activate_color`,
   `remove_empty_layers`). Nie manuelles Layer-Anlegen. **Das war unser Fehler.**
3. **Snapshot-Undo** ist einfacher und robuster als Command-Undo.
4. **Projekt = JSON-Ordner** mit Layer+Shape-Arrays, Bilder daneben.
5. **Job-Pipeline:** Shapes → Ruida-Layer-Dedup → Preamble/Config/Geometrie/
   Trailer → Swizzle → UDP. Cut/Fill/Raster als drei Schnitt-Arten.
6. **Tauri-Muster:** Svelte ruft Rust-Commands (wie web/*.js den Server rief);
   Geometrie/Job im Core, Frontend zeichnet nur.
