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

- geschlossene Konturen eines zusammengesetzten Pfads werden als
  Dreiecksfächer in das Stencil-Paritätsbit geschrieben;
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

Beim Gegencheck mit dem Anker-Asset zeigte sich außerdem ein Importfehler,
nicht eine umgedrehte Even-Odd-Regel: Das SVG enthält ein vollflächiges weißes
Hintergrund-Rechteck, das nach dem bisherigen Verlust der SVG-Farbe als normale
Laserfläche im Layer landete. Der SVG-Import verwirft deshalb reine weiße
Füllpfade ohne Stroke. Weiß gefüllte Formen mit Stroke bleiben als beabsichtigte
Vektorkontur erhalten. Damit bleibt der Raum um den schwarzen Anker frei, ohne
die korrekte Stencil-Füllung des Aztec-Stresstests zu verändern.

Die vollständige Prüfung vom Import bis zum Job-Fill deckte zwei weitere
Informationsverluste auf. Der Import zerlegte jeden SVG-Pfad in nackte
Einzelkonturen; dadurch wurde Even/Odd später global über den ganzen Layer
angewandt. SVG verlangt dagegen Even/Odd innerhalb eines zusammengesetzten
Pfads und anschließend die Vereinigung getrennter gemalter Pfade. Außerdem sind
Teilpfade eines gefüllten SVG-Pfads auch ohne abschließendes `Z` für die
Füllauswertung implizit geschlossen. Genau diese Schlusskonturen fehlen im
Anker-Asset explizit; der alte Import verwarf sie deshalb beim Fill.

`fill_group_id` erhält nun die zusammengesetzten Pfade im Core-Modell. Import,
GPU-Stencil und `JobPlan` verwenden dieselbe Gruppierung: Parität je
Füllpfad, Union zwischen Füllpfaden. Der reale Anker ergibt damit in der
Core-Scanline die erwarteten Stichproben: Auge frei, Schaft gefüllt, Raum um
den Anker frei und Zahnkranz gefüllt. Reine Stroke-Pfade ohne `Z` bleiben offen.

## Nachtrag 2026-07-18: Messbarer nativer Renderpfad

Vor weiteren GPU-Umbauten bekommt der native Renderer eine opt-in Baseline.
Mit

```bash
LUXIFER_RENDER_PROFILE=1 RUST_LOG=luxifer_render_perf=info \
  cargo run --release -p luxifer-native
```

fasst er einmal pro Sekunde folgende CPU-/Treiberwerte zusammen: kompletter
Frame, egui-Aufbau und -Tessellierung, Fill- und Linienaufbereitung,
Overlay-Aufbereitung und Image-Draw-Vorbereitung. Dazu protokolliert er
Szenen-Rebuilds, Scene-/Fill-/Overlay-Vertices, Fill-Compounds und die aus dem
Renderpfad abgeleitete Draw-Call-Anzahl.

Die Werte sind bewusst keine behauptete GPU-Laufzeit: `queue.submit()` ist
asynchron. GPU-Zeitstempel werden erst ergänzt, wenn die CPU-Baseline einen
GPU-seitigen Verdacht belegt. Der nächste Umbau wird anhand dieser Baseline der
Live-Transformpfad für Move ohne vollständigen Scene-Rebuild und Upload.

### Messung und erster Cache-Schritt

Release-Messung am realen punktreichen Belastungsprojekt vom 2026-07-18:

| Zustand | Szene | Auswahl-Overlay | CPU-Overlay/Frame |
|---|---:|---:|---:|
| vorher, große Auswahl | 753.300 Vertices | 753.624 Vertices | ca. 6–9 ms |
| nach persistentem Auswahl-Cache | 753.300 Vertices | 753.228 gecacht + 396 dynamisch | ca. 0,02 ms |

Die Auswahlkonturen liegen nun in einem eigenen persistenten GPU-Buffer. Nur
Auswahlbox, Handles und Werkzeugvorschauen werden weiterhin pro Frame erzeugt.
Damit kostet eine statische große Auswahl nur einen zusätzlichen Draw Call und
keinen erneuten Aufbau samt Upload von rund 753.000 Vertices.

Die anschließende Move-Messung isoliert den nächsten Engpass: je nach
Pointerrate 24–39 vollständige Szenen-Rebuilds pro Sekunde, mit rund
3,6–5,4 ms allein für die Linienaufbereitung pro Rebuild. Der nächste Schritt
bleibt daher der GPU-Live-Transform während Move; erst beim Loslassen wird die
Core-Geometrie endgültig übernommen und der Scene-Cache einmal erneuert.

### GPU-Live-Move

Ungefüllte Vektorauswahlen werden während Move nicht mehr im Core mutiert. Die
selektierten Konturen liegen ausschließlich im Auswahlbuffer und erhalten über
eine zweite Kamera-Uniform den aktuellen Welt-Offset. Auswahlbox und Handles
folgen demselben Offset. Beim Loslassen übernimmt ein einziger Core-Edit die
Gesamtverschiebung; Undo bleibt damit genau ein Schritt. Escape verwirft den
Offset ohne Core-Rollback.

Der Release-Gegencheck am 753k-Vertex-Projekt zeigt während der isolierten
Move-Geste null Rebuilds; beim Loslassen folgt genau ein Scene- und
Selection-Rebuild. Die vorherigen 24–39 Rebuilds pro Sekunde entfallen damit.
Gefüllte Konturen und Bilder bleiben vorerst im inkrementellen sicheren Pfad,
bis Fill- beziehungsweise Image-Geometrie ebenfalls eine selektive
GPU-Transformation besitzt.

### GPU-Live-Move für vollständige Fill-Auswahlen

Der Stencil-Fill kann dieselbe Auswahl-Uniform verwenden, solange der gesamte
sichtbare Fill-Inhalt gemeinsam bewegt wird. Sind alle sichtbaren geschlossenen
Fill-Konturen ausgewählt, bindet der Fill-Pass deshalb während Move die
Selection-Uniform: Stencil-Dreiecke, Compound-Cover und Layer-Cover wandern als
eine Einheit, ohne CPU-Neuaufbau oder Upload. Beim Loslassen folgt wie beim
Linienpfad genau ein Core-Commit.

Eine Teilmenge mehrerer Fill-Konturen bleibt bewusst im bisherigen Pfad. Ein
globaler Offset würde sonst nicht ausgewählte Füllungen mitverschieben; eine
korrekte Beschleunigung dieses Falls benötigt getrennte selektierte und
unselektierte Fill-Batches. Zwei Regressionstests sichern beide Grenzen:
vollständige Fill-Auswahl nutzt GPU-Live-Move, partielle Fill-Auswahl mutiert
weiterhin inkrementell und bleibt visuell korrekt.

Der visuelle Release-Gegencheck am realen Belastungsprojekt bestätigte den
Pfad mit 371.202 Fill-Vertices, 753.228 Auswahl-Vertices und einem
Fill-Compound: während Move null Rebuilds, beim Loslassen genau ein Rebuild.
Kontur, Stencilfläche und Auswahlrahmen folgten gemeinsam dem GPU-Offset.

### Persistenter Image-Quad-Cache

Designbilder erzeugen ihre sechs Quad-Vertices, Asset-Draw-Ranges und den
`wgpu::Buffer` nur noch bei einer Szenenänderung. Der Frame-Pfad bindet den
persistenten Buffer und die bereits vorhandenen Texturen; Pan und Zoom ändern
nur die Kamera-Uniform. Damit entfallen die vorherigen Frame-Allokationen,
Asset-ID-Clones und GPU-Buffer-Erzeugungen.

Der Release-Gegencheck mit einem Image-Shape bestätigte korrekte Anzeige,
Pan/Zoom und Move-Fallback. Die reine Image-Draw-Vorbereitung liegt stabil bei
rund 0,01 ms pro Frame. Ein selektiver GPU-Live-Move für Bilder hat damit
gegenüber Resize/Rotate großer Vektorauswahlen keine aktuelle Priorität.

### GPU-Live-Resize für ungefüllte Vektorauswahlen

Die Selection-Uniform trägt nun eine allgemeine affine 2×2-Transformation mit
Pivot und Offset statt nur einer Translation. Resize geeigneter ungefüllter
Vektorauswahlen aktualisiert während der Geste ausschließlich diese Uniform und
die kleine Ziel-BBox für Handles. Shape-Snapshots, Core-Mutationen sowie Scene-
und Selection-Rebuilds entfallen während der Vorschau; beim Loslassen wird die
Ziel-BBox genau einmal über `scale_edit` übernommen. Escape verwirft die
Vorschau ohne Core-Rollback.

Bilder und Fill-Auswahlen bleiben beim Resize zunächst im bisherigen Pfad. Ein
Regressionstest belegt, dass Core-Revision und Ausgangsgeometrie während der
GPU-Vorschau unverändert bleiben, die affine Matrix die erwartete Skalierung
enthält und der abschließende Commit exakt die Zielbreite erzeugt. Rotate kann
im nächsten Schritt dieselbe Transform-Uniform mit einer Rotationsmatrix nutzen.

### GPU-Live-Rotate für ungefüllte Vektorauswahlen

Rotate nutzt nun dieselbe affine Selection-Uniform mit einer Rotationsmatrix um
den Auswahlmittelpunkt. Während der Geste bleiben Core-Geometrie, Shape-
Snapshots und beide GPU-Caches unverändert. Die sichtbare Ziel-BBox wird aus
den vier um den Pivot rotierten Ecken berechnet, sodass Box und Handles der
Vorschau folgen. Beim Loslassen übernimmt ein einzelner
`rotate_edit_around`-Commit den Gesamtwinkel; Escape verwirft nur die Matrix.

Der Release-Gegencheck mit 753.228 Auswahl-Vertices zeigte während der gesamten
Rotate-Geste null Scene- und Selection-Rebuilds. Am Abschluss folgte genau ein
gemeinsamer Rebuild. Ein Regressionstest sichert eine 90-Grad-Matrix,
unveränderte Core-Revision während der Vorschau und den einmaligen finalen
Rotationswert. Fill-Auswahlen und Bilder bleiben beim Rotate zunächst im
sicheren bisherigen Pfad.

### Korrektur der Rotate-Vorschau

Der erste GPU-Live-Rotate-Gegencheck zeigte drei getrennte Darstellungsfehler:

- Vertexpositionen rotierten, die Segmentrichtung für die shaderseitige
  Linienbreite jedoch nicht. Dadurch kollabierten Linienquads winkelabhängig
  und punktreiche SVGs wirkten, als würden sie ausfaden.
- Die Live-Auswahl zeigte die achsenparallele Hüllbox der rotierten Box und
  wuchs deshalb bei Zwischenwinkeln. Rahmen und Handles rotieren nun als
  orientierte Ausgangsbox gemeinsam um denselben Pivot.
- Der persistente Image-Quad-Cache verwendete nur `x/y/w/h` und ignorierte
  `Shape.rotation`. Bild-Quads rotieren nun samt UV-Koordinaten um ihren
  Mittelpunkt; der sichere Image-Rebuild-Pfad zeigt Bild und Rahmen gemeinsam.

Der Shader transformiert jetzt neben der Position auch `dir` mit der affinen
2×2-Matrix und normalisiert die Richtung vor der Linienextrusion. Release-
Gegenchecks bestätigten den SVG-GPU-Pfad ohne Zwischen-Rebuilds und zunächst
den korrekten Image-Rebuild-Pfad bei rund 0,01 ms Image-Draw-Zeit pro Frame.

### GPU-Live-Transformation für Bilder und vollständige Fill-Auswahlen

Die Bild-Pipeline besitzt nun wie die Vektor-Pipeline getrennte Kamera- und
Selection-Uniforms. Der gecachte Quad-Buffer bleibt während Move, Resize und
Rotate unverändert; nur ausgewählte Bild-Ranges binden die affine
Selection-Uniform. Beim Loslassen übernimmt weiterhin genau ein Core-Commit
die endgültige Geometrie. Ein Release-Lauf mit echtem PNG validierte die
WGSL-Pipeline und den Texturpfad bei rund 0,01 ms Image-Draw-Zeit pro Frame.

Der Stencil-Fill-Pfad konnte dieselbe affine Uniform bereits anwenden, war aber
auf reine Translation beschränkt. Resize und Rotate nutzen sie jetzt ebenfalls,
wenn alle sichtbaren Vektor-Fill-Konturen ausgewählt sind. Nur dann darf der
gemeinsame Fill-Buffer vollständig transformiert werden. Sobald lediglich ein
Teil der sichtbaren Fills ausgewählt ist, bleibt der Snapshot-/Core-Pfad aktiv,
damit unselektierte Compounds nicht mitbewegt werden.

Regressionstests sichern für Bild-Rotate und vollständiges Fill-Resize: keine
Core-Revision und keine Geometriemutation während der Vorschau, affine
GPU-Matrix während der Geste und genau ein finaler Commit.

### Fill-Compound-Stresstest nach dem Transform-Checkpoint

Ein synthetischer Release-Test bildet die historische Obergrenze mit 1.808
getrennten geschlossenen Fill-Shapes nach. Ergebnis:

- 1.808 Fill-Compounds auf einem Layer,
- 21.702 Fill-Vertices,
- 0,751 ms CPU-Aufbauzeit,
- 5.426 Fill-Draw-Calls im aktuellen Stencilpfad.

Die CPU-Aufbereitung ist damit klar unkritisch. Das verbleibende Risiko liegt
im GPU-/Treiber-Overhead der drei Stencil-Draws je Compound. Die Compounds
dürfen nicht einfach gemeinsam per Even-Odd behandelt werden: getrennte
gemalte SVG-Füllpfade werden vereinigt, während Even-Odd nur innerhalb eines
Compounds gilt. Der dauerhafte Test sichert deshalb Compoundzahl, Vertexzahl
und Draw-Call-Ableitung. Vor einem semantischen Umbau folgt ein gezielter
GPU-Benchmark dieses 5.426-Draw-Falls.

### GPU-Gegencheck des 5.426-Draw-Falls

Der native Startpfad kann die synthetische Szene opt-in direkt in den
produktiven Surface-, Stencil-, MSAA- und Present-Pfad einsetzen:

```bash
LUXIFER_FILL_STRESS=1808 \
LUXIFER_RENDER_PROFILE=1 \
RUST_LOG=luxifer_render_perf=info \
cargo run --release -p luxifer-native
```

Ohne `LUXIFER_FILL_STRESS` bleibt der normale Projektstart unverändert. Der
Release-Gegencheck auf dem vorhandenen RADV/Vulkan-System ergab:

- 1.808 Fill-Compounds und 5.429 geschätzte Canvas-Draws insgesamt,
- stabile 60-Hz-Intervalle mit etwa 16,18 bis 16,34 ms pro Frame,
- etwa 0,17 bis 0,18 ms UI und 0,13 bis 0,14 ms egui-Tessellation,
- einmalig 0,84 ms Fill- und 1,70 ms Linien-Cacheaufbau,
- danach keine Scene- oder Selection-Rebuilds.

Damit ist die Compoundzahl weiterhin ein Skalierungsrisiko für schwächere
Treiber und höhere Bildraten, auf der gemessenen Hardware aber kein belegter
60-Hz-Flaschenhals. Ein semantisch riskantes Zusammenlegen unabhängiger
Compounds oder ein neuer Fill-Renderer ist derzeit nicht gerechtfertigt. Der
opt-in Hook bleibt als reproduzierbarer Regressionstest erhalten.

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
