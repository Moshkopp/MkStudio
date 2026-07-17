# luxifer-native — nativer Editor (winit + wgpu + egui)

Der Umbau aus **ADR 0010**: die Zeichen-/UI-Schicht von LuxiFer nativ, ohne
WebView, ohne IPC. `luxifer-core` bleibt die einzige Quelle der Wahrheit und wird
direkt gelinkt. Läuft **neben** der weiter funktionierenden Tauri-App
(`luxifer/frontend/`), damit man vergleichen kann.

## Warum

Direkter Release-Vergleich mit ThorBurn (Qt) zeigte LuxiFer auf allen Achsen
träge. Ein wgpu-Spike bewies: derselbe Core + dieselbe Aztec-Geometrie laufen
nativ mit 144 fps, während die Tauri-App kriecht — der Engpass ist die
IPC-Brücke (73k Segmente Rust→JSON→JS) plus WebKitGTK-Overhead, nicht der Core
und nicht die GPU. Details: `docs/adr/0010-nativer-renderer-wgpu.md`.

## Starten

```bash
# aus der Repo-Wurzel
GDK_BACKEND=x11 cargo run -p luxifer-native --release

# mit direkt geladener Datei (erstes Argument):
GDK_BACKEND=x11 cargo run -p luxifer-native --release -- /pfad/zu/datei.svg
```

Ein versioniertes Linux-AppImage inklusive Release-Build entsteht mit:

```bash
./scripts/build-appimage.sh
```

Das Ergebnis liegt unter `dist/LuxiFer-<version>-<architektur>.AppImage`.

`GDK_BACKEND=x11` aus demselben Grund wie in `dev.sh` (Wayland-Present-Latenz).

### Test-Umgebungsvariablen
- `LUXI_FILL=1` — beim Auto-Import gleich alle Layer auf Fill stellen.
- `LUXI_TAB=laser` — rechten Reiter direkt auf Laser starten.

## Was läuft (Stand: Umbau-Branch)

- **Canvas** (wgpu): Shapes aus dem echten `AppState`, Tisch-Rahmen, Auswahl-
  Hervorhebung + BBox, **Transform-Handles** (8 Skalier + Rotate). Pan (mittlere
  Maus / Leertaste+links), Zoom (Mausrad, auf den Cursor).
- **Dicke Linien**: Konturen/Handles als bildschirm-konstant dicke Linien
  (Segment→Quad, Dicke im Screen-Space-Shader), nicht mehr 1px-aliast.
- **Design-Flächen-Fill** als GPU-Even-Odd-Stencil-Fläche, unabhängig vom
  Laser-Zeilenabstand. Scanlines entstehen nur für Job und Laserpreview;
  Pan/Zoom und Transform des Aztec-Stresstests bleiben dadurch leichtgewichtig.
- **Import**: SVG/DXF (`import_vector`) + **Bilder** (`import_image`, als
  GPU-Textur gerendert) + **Text→Pfad** (`text_to_contours`, System-Font-Wahl).
- **Interaktion** über den Core: Rechteck/Ellipse/Polygon zeichnen, Auswahl +
  Hit-Test, Verschieben, **Resize/Rotate** über Handles, Marquee, Farbe/Layer,
  Undo/Redo, Löschen.
- **Panels** (egui, Tauri-nahes Theme): Werkzeuge links; rechts Ebenen + Palette
  (aktive-Farbe-Markierung) bzw. **Laser** (Ampel-Grid, echter Treiber Ruida/GRBL:
  Start/Pause/Stopp/Frame/Export, Jog/Home, Job-Parameter, Profil-Dialog).
- **Reiterleiste** oben (Projekt / Design / Laser). **Projekt-Browser**: Liste,
  Neu, Öffnen, Speichern (Strg+S) / neue Version (Shift+Strg+S) — Core-Projekt-API.
- Tastatur: V/R/E/P Werkzeuge, Z/Y Undo/Redo, Strg+S speichern, Entf löschen,
  Esc abbrechen, Enter Polygon schließen.

## Was noch fehlt / offen

- **Fenster-Sichtbarkeit**: Erster Frame wird sofort präsentiert (Wayland-Fix);
  vom Nutzer als funktionierend bestätigt. Falls das Fenster mal leer bleibt,
  hilft `WGPU_BACKEND=vulkan`.
- **Echtes MSAA** (aktuell dicke Quads statt Anti-Aliasing der Kanten).
- **Vorschau-Simulation/Scrubber** und ein **Monitor-Reiter** (niedrige
  Priorität); die statische treiberautoritative Laser-Vorschau ist vorhanden.
- **Bézier-Node-Editing** in der nativen UI vervollständigen.
- **Trim-Werkzeug** als echte Geometrieoperation; derzeit nur ausgegrauter Stub.
- **Gespeicherte Projekt-/Version-Thumbnails**; Browser, Versionsliste und
  Live-Vektor-Miniatur sind vorhanden.
- **Laser-Ping/Position** und Ruida-Geräteabnahme. Die GRBL-Abnahme bleibt bis
  zur Verfügbarkeit passender Hardware zurückgestellt.

## Architektur (Module)

- `main.rs` — winit-Loop.
- `app.rs` — hält `AppState` + Kamera + View/Tool-Zustand, verbindet Eingaben mit
  Core-Aufrufen, rendert Canvas + Bilder + Overlay + egui in einen Frame.
  Vertex-Cache, Transform-Handles, Import/Text/Projekt-Methoden.
- `gpu.rs` — wgpu-Setup, Linien-Pipeline (dicke Quads), Kamera-Uniforms, Overlay.
- `image_gpu.rs` — Bild-Textur-Pipeline (Assets als texturierte Quads).
- `camera.rs` — Welt(mm)↔Bildschirm(px), Pan/Zoom/Fit.
- `scene_geo.rs` — `AppState` → Vertices (Konturen, Scanline-Fill, Handles).
- `ui.rs` — egui-Panels (Reiterleiste, Werkzeuge, Ebenen, Palette, Projekt-
  Browser, Laser-/Text-Dialoge, Theme).
- `laserpanel.rs` — Laser-Bedienpanel (Ampel-Grid, echter Treiber).
- `laser.rs` — Laser-Backend (Registry + Treiber, Aktionen, Export).
- `project.rs` — Projekt-Backend (öffnen/speichern/Versionen).
- `fonts.rs` — System-Font-Scan fürs Text-Werkzeug.
- `tools.rs` — Werkzeug-/View-/Laser-UI-Zustand.

**Invariante bleibt gewahrt:** keine Fachlogik hier — alles Mutierende geht durch
`luxifer-core` (199 Core-Tests unberührt; native Verdrahtung mit 8 eigenen Tests).
