# ADR 0008: Render-Architektur — GPU statt CPU-Canvas

## Status
Akzeptiert — 2026-07-10

## Kontext

LuxiFer fühlt sich **träge** an, und das Gefühl wird mit jedem Feature schlimmer.
Der Nutzer hat es benannt: „im Canvas ist alles CPU, was von vornherein GPU sein
sollte" — und der Vergleich sticht: die alte ThorBurn-Version (Qt/QML, also
GPU-beschleunigt) war **~50× performanter** als das heutige LuxiFer, und daran
ändert auch ein Release-Build nichts.

Ausgelöst wurde die Analyse vom Bild-Rastern (ADR 0004 §5, jetzt umgesetzt): ein
Ausmalbild erzeugt bei 0,1 mm Zeilenabstand **~79.000 Rasterzeilen-Runs**. Die
Laser-Preview (ADR 0005) rendert jedes Segment einzeln mit `ctx.stroke()` — das
bricht sichtbar zusammen (Laden mehrere Sekunden, jeder Zoom-Tick zäh). Das ist
aber **kein Raster-Sonderproblem**, sondern legt eine Grundsatz-Schwäche offen,
die auch den Design-Canvas betrifft: **beide Canvasse rendern mit der Canvas-2D-
API (`getContext("2d")`), also auf der CPU.**

### Messung (Beleg statt Vermutung)

80.000 Liniensegmente (≈ ein gerastertes Bild), 60 Redraws (simuliert Pan/Zoom),
gemessen im echten Browser auf der Zielhardware:

| Methode | ms/Frame | 60 fps (< 16,7 ms)? |
|---|---|---|
| **Canvas 2D, `stroke()` pro Segment** (= Preview heute) | **40,8** | ❌ (~24 fps) |
| Canvas 2D, ein Pfad gebatcht (eine Farbe) | 3,3 | ✅ |
| **WebGL, ein Draw-Call** | **~0** | ✅✅ |

Der Core-Anteil ist **nicht** das Problem: der `JobPlan`-Aufbau für dasselbe Bild
dauert im Release ~50 ms **einmalig**; die 40,8 ms fallen **pro Frame** an (bei
jedem Pan-Pixel). Auch Tauri-IPC ist nicht der Flaschenhals — die Zeit steckt im
**Zeichnen**. Damit ist die Ursache belegt: **CPU-Canvas ist die Wurzel der
Trägheit; die GPU erledigt dieselbe Last in ~0 ms** (deckt sich mit „Qt war 50×").

Zweite Messung (rohes WebGL, Skalierung + Per-Segment-Farbverlauf — genau der
Reihenfolge-Verlauf, der das CPU-Batching ausschließt):

| Segmente | WebGL einfarbig | WebGL mit Per-Segment-Farbverlauf |
|---|---|---|
| 80.000 | < 0,1 ms | < 0,1 ms |
| 300.000 | < 0,1 ms | < 0,1 ms |
| 1.000.000 | < 1 ms | < 1 ms |

Rohes WebGL trägt **1 Mio** Segmente mit vollem Farbverlauf weit unter der
60-fps-Grenze. Der Verlauf kostet auf der GPU **nichts extra** — er ist nur ein
Vertex-Attribut im Buffer. Damit steht auch die Lib-Frage auf Fakten (siehe §1).

### Warum das jetzt entschieden wird

Die Trägheit vergiftet jede weitere Arbeit: An einem zähen Canvas macht weder
Feature- noch UX-Arbeit Fortschritt sichtbar, und jedes neue Feature packt mehr
Last auf denselben CPU-Pfad. Bevor der Workflow (Panels, Reiter — bewusst
**eigene, spätere** ADR) verbessert wird, muss das Fundament schnell sein.

### Wie es dazu kam (dokumentierte Lehre)

Diese Entscheidung kommt zu spät, und das gehört ehrlich festgehalten, damit sie
sich nicht wiederholt. Das CPU-Rendering (`getContext("2d")`) war **nie bewusst
gewählt** — es war der Default-Pfad im WebView, der bei den wenigen Vektor-Formen
der frühen Tests „funktionierte". Die Performance-Frage wurde damit an genau der
Stelle **nicht** gestellt, an die sie gehört: **bevor** die Render-Technik in
ADR 0005 (Preview) und im Design-Canvas festgelegt wurde. Eine Render-Architektur
(CPU vs. GPU) ist ein **Fundament**, keine spätere Optimierung — sie fällt nicht
unter „erst am fertigen System optimieren" (das meint Mikro-Tuning, nicht die
Technologiewahl).

Verschärfend: Eine **direkte Rückfrage des Nutzers**, welche Option performanter
sei, blieb unbeantwortet — statt zu **messen**, wurde die vermutete Antwort
weitergebaut. Das Messen war billig (ein Browser-Benchmark, ~20 Minuten, siehe
Tabelle oben) und hätte die Entscheidung sofort geklärt. Die Kosten des
Nicht-Messens waren Tage Frust an einem Symptom, dessen Ursache eine einzige Zahl
offengelegt hätte.

**Regel daraus:** Architektur-/Render-Weichen werden **gemessen, bevor gebaut
wird**; eine direkte Performance-/Optionsfrage wird **mit einer Messung
beantwortet**, nicht mit einer Annahme überbaut.

## Entscheidung

**Grundsatz: Alles, was gezeichnet wird, rendert auf der GPU — nicht auf der
CPU.** Das gilt für **jede** Ansicht: Design-Canvas, Laser-Preview und die
Laser-/Live-Ansicht. Es ist keine canvasweise Nachrüstung, sondern ein Prinzip —
CPU-`ctx.stroke()`/Canvas-2D ist als Zeichenmittel **nicht mehr erlaubt**. Der
Core bleibt unberührt — er liefert weiterhin die geräteunabhängige Wahrheit
(`JobPlan`/`JobPreview` in mm); nur die **Zeichenschicht** im Frontend wechselt
die Technologie (CLAUDE.md Regel 1 & 2 bleiben: Fachlogik im Core, Frontend
zeichnet nur).

### 1. EINE gemeinsame GPU-Render-Schicht für alle Ansichten

Nicht drei Canvasse mit je eigenem Zeichencode (heute rechnet jeder seine eigene
mm→Pixel-Transformation, Kamera, Grid), sondern **eine** GPU-Render-Schicht
(WebGL), die Design, Preview und Laser-Ansicht **gemeinsam** nutzen. Das ist
schneller *und* sauberer: eine Kamera-Logik, ein Satz Primitive, ein Ort für
Kontextverlust-Behandlung — statt dreifach dieselbe Rechnerei.

Die Schicht wird hinter einer **kleinen internen Render-API** gekapselt (z. B.
`drawLines(segments, colors)`, `drawImageRaster(...)`, Kamera/Transform), damit
der Rest des Frontends nicht in WebGL-Details ertrinkt und ein späterer Wechsel
(WebGPU) lokal bleibt.

**Umsetzung = rohes WebGL, dünn selbst gekapselt — keine Render-Lib.** Das ist
die *performanteste* Wahl, belegt durch die Messung: rohes WebGL trägt 1 Mio
Segmente mit Farbverlauf < 1 ms. Libs wie regl oder PixiJS sind nur **dünne
Schichten über genau diesem WebGL** — sie können prinzipiell **nicht schneller**
sein (bestenfalls gleich, mit etwas Overhead), bringen aber **Gewicht + eine
Abhängigkeit** mit (PixiJS ist groß). Das widerspricht dem Projektprinzip
„offline-first, wenig Gewicht". Da rohes WebGL die Last mühelos trägt, gibt es
**keinen Performance-Grund** für eine Lib — nur Bequemlichkeit, die den Preis
nicht wert ist. Die eigene dünne Kapselung ist der beste Punkt auf der Kurve:
maximale Performance, minimales Gewicht, volle Kontrolle. (Sollte die Kapselung
wider Erwarten ausufern, ist eine schlanke Lib wie *regl* — nicht das schwere
PixiJS — der Fallback; aber der Default ist rohes WebGL.)

### 2. Bild-Vorschau: Gesamtbild rausgezoomt → einzelne Rasterzeilen reingezoomt

Das gewünschte Verhalten (Vorbild LightBurn) ist mit GPU das **natürliche**
Ergebnis, nicht ein Extra-Feature:

- **Rausgezoomt** verschmelzen die dichten Rasterzeilen optisch zu einer sauberen
  **Fläche** — man sieht *das Bild*, wie es gebrannt aussieht.
- **Reingezoomt** trennen sich die **einzelnen Pfade je Rasterzeile** sichtbar
  auf, bis bei starkem Zoom große Zeilenabstände und die **Kantensauberkeit** der
  Runs erkennbar sind.

**Bild-Layer werden als GPU-Textur gezeichnet, nicht als Segmente.** Messung
(dieses Setup): ein reales Ausmalbild erzeugt **~445.000** Rasterzeilen-Runs. Die
als einzelne `PreviewMove`-Segmente durch IPC zu schicken und im JS zu Vertex-
Arrays zu bauen kostet mehrere Sekunden *pro Laden* (1,5 s IPC/Core + 2 s
Array-Aufbau) — unabhängig vom Zeichnen, das dank GPU flüssig ist. Ein Bild ist
kein Haufen Linien; die richtige Datenform für die Anzeige ist eine **Textur**.

Wichtig — das ist **nicht** das in dieser Session verworfene Bitmap:

- **Verworfenes Bitmap:** ein fertig gerendertes PNG in *Anzeige*-Größe → beim
  Reinzoomen matschig (feste Auflösung).
- **Diese Textur:** **ein Texel pro Rasterzelle** (native Job-Auflösung, aus
  denselben `LayerWork::Raster`-Zeilen wie der Job), gezeichnet mit `NEAREST`-
  Sampling. Rausgezoomt = saubere Fläche; reingezoomt werden die Texel scharf
  größer → man sieht die **einzelnen Rasterzeilen** als scharfe Pixelreihen.

Sie **lügt nicht** (anders als „Runs zusammenfassen", das bewusst verworfen
wird): jeder gebrannte Texel ist gesetzt, jeder nicht-gebrannte leer — exakt das
Brennergebnis, nur als Pixel statt als Striche. Eine Wahrheit mit dem Job bleibt.

**Der Core baut die Textur, das Frontend rechnet nichts** (CLAUDE.md Regel 1).
Die Bild-Pixel (ein Texel je Rasterzelle) werden **im Core** aus den
`LayerWork::Raster`-Zeilen erzeugt und dem Frontend als kompakte Bytes + Tisch-
Position/Maße geliefert. Das Frontend lädt sie nur per `texImage2D` hoch und
zeichnet ein texturiertes Quad — **keine Pixelrechnerei im Canvas**. Das ist
bewusst anders als ältere Ansätze (Referenzversionen), die das Bitmap im
Frontend „backten": Bildaufbau ist Fachlogik und gehört in den Core. Der IPC
transportiert dann eine kompakte Textur statt Hunderttausender Segmente — das
löst die gemessene Lade-/Transferzeit.

**Cut/Fill bleiben Segmente** (davon gibt es wenige) und werden weiter als Linien
gezeichnet — mit vollem **Scrubber/Play** (ADR 0005 §4), wo er Sinn ergibt
(überschaubare Konturen, Reihenfolge wichtig). Für ein Rasterbild mit
Hunderttausenden Zeilen ist ein Move-für-Move-Scrubber ohnehin sinnlos.

### 3. Revision von ADR 0005 (Preview)

ADR 0005 bleibt in seiner **Architektur** gültig (Preview = Visualisierung des
`JobPlan`, eine Wahrheit, Ableitung im Core). Revidiert wird die **Render-Technik**
der Umsetzung: Cut/Fill-`PreviewMove`s werden **nicht** mehr als einzelne
CPU-`stroke()`-Aufrufe gezeichnet, sondern über die GPU-Render-Schicht (§1).
Bild-Layer erscheinen **nicht** als `MoveKind::Raster`-Segmente in der Preview,
sondern als **Textur** (§2) — die Rasterzeilen bleiben im `JobPlan`/Job die
Wahrheit, die Preview zeigt sie nur als Pixel statt als Hunderttausende Segmente.

### 4. Reihenfolge-Einfärbung GPU-tauglich

Der Reihenfolge-Farbverlauf (ADR 0005 §4, früh kühl → spät warm) darf das GPU-
Batching **nicht** wieder zu Pro-Segment-Aufrufen zwingen. Umsetzung: die
`seq`-Farbe als **Vertex-Attribut** (pro Segment eine Farbe im Buffer) — dann
bleibt es **ein** Draw-Call trotz Verlauf. Kein Rückfall auf CPU-Buckets.

## Invarianten

1. **Core bleibt die Wahrheit, Frontend zeichnet nur** (CLAUDE.md Regel 1 & 2).
   Der Wechsel betrifft ausschließlich die Zeichenschicht; `JobPlan`/`JobPreview`
   und die mm-Koordinaten im Core ändern sich nicht.
2. **Alles Gezeichnete läuft über die GPU.** Jede Ansicht (Design, Preview,
   Laser/Live) rendert über die gemeinsame GPU-Schicht; Pan/Zoom/Scrub müssen bei
   zehntausenden Segmenten flüssig (< 16,7 ms/Frame) bleiben. CPU-Canvas-2D
   (`ctx.stroke()`/`fill()` etc.) ist als Zeichenmittel nicht mehr erlaubt — kein
   neues Feature darf darauf zurückfallen.
3. **Kein separates Vorschau-Bitmap.** Bilder werden aus ihren echten
   Rasterzeilen gezeichnet (scharf auf jeder Zoomstufe), nie als vorgerendertes
   PNG — sonst brechen Zoom-Schärfe und Scrubber.
4. **Offline-first.** Keine CDN-/Netz-Abhängigkeit der Render-Schicht (Fonts,
   Libs, Shader werden mitgeliefert).
5. **Eine Wahrheit für Vorschau und Job** (ADR 0005): Was die GPU zeichnet, sind
   dieselben Daten, die der Treiber kompiliert.

## Konsequenzen

- Neue **gemeinsame Render-Schicht** im Frontend (GPU-gekapselt) mit Primitiven
  für Linien (mit Per-Segment-Farbe), Bett/Raster/Marker, Bilder und Kamera
  (mm→Clip) — **von Design, Preview und Laser-Ansicht geteilt genutzt**, nicht pro
  Canvas dupliziert.
- **PreviewCanvas** wird auf die Render-Schicht umgestellt; der Reihenfolge-
  Verlauf wird als Vertex-Farbe realisiert (ein Draw-Call). Der Scrubber (ADR
  0005 §4) bleibt möglich.
- **Design-Canvas** (Canvas.svelte) wird ebenfalls umgestellt — er leidet unter
  derselben CPU-Grenze (die vorhandene „drawImage statt Neurendern"-Optimierung
  ist ein Symptom davon). Umstellungs-Reihenfolge nur pragmatisch: Preview zuerst
  (dort ist der Schmerz am größten), Design-Canvas direkt danach — das Ziel ist,
  dass **keine** Ansicht mehr auf CPU zeichnet.
- **WebGL-Kontextverlust** muss behandelt werden (Ressourcen neu aufbauen) — eine
  Klasse Fehler, die es bei Canvas 2D nicht gab.
- Kein Core-Umbau, keine Treiber-Änderung; die ADR ist rein Frontend-seitig.

## Nicht Teil dieser Entscheidung

- **UX / Workflow / Panels / Reiter** — das zweite große Grundsatzthema
  („zu viele Reiter/Modi", „Panels weit weg von schönem Arbeiten", der Bruch
  Bild→Laser). Bewusst **eigene, spätere ADR**: erst das schnelle Fundament
  (dieses ADR), dann der Workflow darauf. Reihenfolge vom Nutzer so gewählt.
- **WebGPU** — mögliche spätere Ablösung von WebGL; die Render-API (§1) kapselt
  den Wechsel, aber jetzt ist WebGL das Ziel (breiter verfügbar im WebView).
- **Wechsel weg von Tauri/WebView** — nicht Gegenstand; die Messung zeigt, dass
  die GPU *im* WebView die Last trägt (WebGL ~0 ms). Kein Fundamentwechsel nötig.
- **Animierte Wiedergabe / Zeit-Schätzung** (ADR 0005 „Nicht Teil") bleiben davon
  unberührt; GPU macht sie eher leichter.
