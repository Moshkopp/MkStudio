# ADR 0010: Nativer Renderer (winit + wgpu) statt Tauri/WebKit

## Status
Akzeptiert — 2026-07-12. Umsetzung im Branch `umbau/wgpu-nativ`.

Löst die Render-Plattform aus ADR 0008/0009 ab: deren Diagnose („GPU statt CPU")
war richtig, blieb aber **im WebView gefangen**. Die Fachlogik (`luxifer-core`)
bleibt unberührt; abgelöst wird nur die Zeichen-/UI-Schicht.

## Kontext

Trotz ADR 0008 (GPU-Canvas) und 0009 (WebGL-only) fühlt sich LuxiFer im
**Release** weiter spürbar träger an als ThorBurn (Qt/QML). Der Nutzer hat es
Seite an Seite verglichen: ThorBurn lädt dieselbe große Aztec-Datei „ohne ein
Augenzwinkern" und füllt sie sofort; LuxiFer braucht Sekunden. Betroffen sind
**alle** Achsen: App-Start, Eingabe-Latenz, Canvas-Pan/Zoom, schwere Ops.

Das ist kein Nachtuning-Thema, sondern eine Architektur-Weiche — also **gemessen,
nicht vermutet** (die Regel aus ADR 0008).

### Messung: Wegwerf-Spike, echter Core, nativ wgpu

Ein isolierter `winit + wgpu`-Spike lädt die reale Aztec-Datei über **denselben**
`luxifer-core`-Pfad wie die App (`import_vector` → `JobPlan::from_shapes` →
`JobPreview::from_plan`), lädt Cut+Fill als GPU-Buffer und pant/zoomt nativ.
Zielhardware (AMD RX 9060 XT), Release:

| Phase | Zeit |
|---|---|
| `import_vector` (SVG parsen) | ~17 ms |
| `JobPlan::from_shapes` (Fill berechnen, Zeilenschritt 0,3 mm) | ~126 ms |
| `JobPreview::from_plan` | ~2 ms |
| **Summe Core (einmalig)** | **~145 ms** |
| Ergebnis | 1.808 Konturen → **73.420 Fill-Segmente** |
| **Pan/Zoom der gefüllten Fläche** | **144 fps, smooth, kein Ruckeln** |

### Schlussfolgerung

- Der **Core** ist schnell (145 ms), die **GPU** langweilt sich (fps durch den
  Monitor gedeckelt, nicht durch Last). Derselbe Datensatz, der die Tauri-App
  zum Kriechen bringt, läuft nativ mit 144 fps.
- Der Engpass liegt damit **weder im Core noch im Rendering**, sondern in der
  **Schicht dazwischen**: 73k Segmente Rust → JSON → Tauri-IPC → JS →
  WebGL-Buffer. ADR 0008 hatte IPC fürs *Pan/Zoom-Frame* zu Recht ausgeschlossen
  (da wird nichts neu übertragen) — beim **initialen Laden** ist die Brücke aber
  genau der Flaschenhals.
- Dazu kommen die strukturellen WebKitGTK-Kosten (App-Start, Present-Latenz —
  siehe `dev.sh`-Notiz), die kein Renderpfad im WebView wegoptimiert.

## Entscheidung

Der Design- und Vorschau-Canvas samt UI wird **nativ** neu gebaut:

- **Fenster/Events:** `winit`
- **Canvas:** `wgpu` (nativer GPU-Present, kein WebView)
- **Panels/UI:** `egui` (oder gleichwertig) — reines Rust
- **`luxifer-core` bleibt die einzige Quelle der Wahrheit** und wird direkt
  gelinkt (kein IPC, keine Serialisierung mehr für Geometrie).

Was **raus** fliegt: der gesamte 2D/WebKit/Svelte-Zeichenpfad
(`luxifer/frontend/src/`, das Tauri-`src-tauri`-WebView-Setup, Canvas-WebGL).

Was **bleibt** unangetastet: `luxifer-core` (Datenmodell, Geometrie, Fill,
Import, Job, Undo, Laser, 199 Tests) und die Treiber. Das ist die
Architektur-Invariante „Frontend zeichnet nur" aus CLAUDE.md — genau sie macht
diesen Wechsel bezahlbar: migriert wird nur die dünne Zeichenschicht.

## Konsequenzen

**Gut:**
- Nativer GPU-Pfad wie Qt; die 144-fps-Messung ist der Beleg.
- Kein IPC/JSON für Geometrie mehr — das initiale Laden großer Dateien fällt von
  Sekunden auf ~Core-Zeit (145 ms).
- Ein Sprache (Rust) durch den ganzen Stack; keine WebKit-Wayland-Workarounds.

**Kosten / Risiken:**
- Die UI-Schicht (Panels, Dialoge, Text-Edit, Werkzeuge, Bild-Import-Vorschau,
  Laserpanel, Projekt-Browser) muss nativ neu gebaut werden — real mehrere Tage.
- egui ist immediate-mode; das frisch gebaute Laserpanel/Palette-Design muss
  übersetzt werden (Layout-Ideen bleiben, Code nicht).
- `main` bleibt lauffähig (Tauri), bis der Branch trägt — kein Big-Bang.

## Nachtrag 2026-07-17: Design-Fill ist keine Laser-Rasterung

Ein gefülltes großes SVG – reproduziert mit dem realen `Aztec.svg` mit 1.808
Konturen – wurde bei Transform-Gesten erneut unbedienbar. Der Design-Canvas
erzeugte bei jeder Geometriemutation die vollständigen Laser-Scanlines neu und
lud für den Stresstest rund 73.420 Liniensegmente als dicke Quads hoch. Diese
Kopplung war auch fachlich falsch: Im Editor soll ein gefülltes SVG als normale
Fläche erscheinen; erst Jobplanung und Laserpreview müssen den realen
Zeilenabstand zeigen.

Der Design-Canvas verwendet deshalb eine GPU-basierte Even-Odd-Stencil-Füllung:

- geschlossene Konturen eines Layers werden als einfache Dreiecksfächer in das
  Stencil-Paritätsbit geschrieben;
- ein Farbpass füllt anschließend nur Pixel mit ungerader Parität;
- Löcher, verschachtelte Konturen und die bisherige Even-Odd-Semantik bleiben
  erhalten, ohne die Konturen auf der CPU zu triangulieren;
- der Design-Fill ist unabhängig von `line_step_mm`;
- offene Konturen und Bildlayer erzeugen keine Designfläche;
- Scanlines bleiben unverändert im `JobPlan`, in der treiberautoritativen
  Ausführungsspur und in der Laserpreview.

Der Frame ist dafür in drei kompatible GPU-Pässe getrennt: Untergrund/Bilder,
Stencil-Flächen und zuletzt Konturen/Overlay/egui. Das reale Aztec-Asset startet
im optimierten Vulkan-Pfad ohne wgpu-Validierungsfehler.

## Offen (Reihenfolge im Branch)

Die funktionale Migration und der vollständige Tauri-Abbau werden durch
[ADR 0011](0011-native-only-anwendungsschicht-und-tauri-abbau.md) präzisiert.
Die operative Reihenfolge und Abnahmekriterien stehen in
[`docs/native_only_migration_tasks.md`](../native_only_migration_tasks.md).

1. Fenster + wgpu-Canvas mit Core-Geometrie (Design-Canvas: Shapes, Pan/Zoom,
   Auswahl, Hit-Test über den Core).
2. egui-Panels: Werkzeuge, Layer, Palette, Laserpanel.
3. Interaktion: Zeichnen, Transform, Text, Import.
4. Laser-Vorschau + Treiber-Anbindung.
5. Projektformat/Assets (unverändert im Core) verdrahten.
