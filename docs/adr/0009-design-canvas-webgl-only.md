# ADR 0009: Design-Canvas — reiner WebGL-Renderpfad (Füllung via Stencil, Core-Triangulierung als Fallback)

## Status
Akzeptiert und umgesetzt — 2026-07-11

## Kontext

Der Design-Canvas ruckelt beim **Verschieben und Skalieren punktreicher Shapes** —
konkret bei Text in schnörkeligen Schriftarten. Ein Textblock ist keine einzelne
Form, sondern zerfällt in viele Konturen (Beispiel „Ich Kack Ab": **92 einzelne
Shapes**, je Buchstabe Außenkontur + Löcher).

Ein zweiter, härterer Belastungsfall bestätigt die Größenordnung: ein reales
**DXF im Azteken-/Star-Wars-Stil** (12,4 MB, `SW_acteken.dxf`), **Gemessen** (Core-Import, Release):

- **1.808 geschlossene Konturen, 125.305 Punkte**, alle auf **einem** Layer.
- Import (Core) **254 ms einmalig** — unkritisch.

Das ist die realistische Obergrenze, an der sich der Renderpfad beweisen muss —
nicht die 92 Textkonturen. Für WebGL-**Linien** ist das harmlos (ADR 0008 maß 1
Mio Segmente < 1 ms). Für die **Füllung** (§1) ist es der eigentliche Stresstest:
1.808 Ringe gemeinsam Even-Odd zu triangulieren (s. Risiko).

Die Ursache des Ruckelns wurde **gemessen, nicht vermutet** (der Weg, den ADR
0008 zur Regel gemacht hat), mit einer temporären Frame-Zeit-Messung im
laufenden Dev-Build. Die erste Grobmessung zeigte nur auf die 2D-Ebene; eine
anschließende Messung je Overlay-Abschnitt isolierte den Verursacher:

| Renderpfad beim Move | ms/Frame | Bewertung |
|---|---:|---|
| **WebGL-Konturen** (nach Offset-Uniform-Fix, s. u.) | **0,00** | ✅ die GPU trägt es mühelos |
| **2D-Canvas-Overlay gesamt** | **221,95** | ❌ ca. 4,5 FPS |
| davon **Auswahl/Gruppenumrisse** | **221,20** | ❌ praktisch die gesamte Frame-Zeit |
| Overlay ohne Gruppenumrisse während der Geste | **0,25** | ✅ flüssig |

Der entscheidende Befund: Die **GPU ist nicht das Problem** — die Konturen kosten
beim Verschieben null. Es ruckelte, weil ein **zweiter Renderpfad** existiert: der
2D-Canvas mit Füllung, Auswahl-Umriss und Knoten. Der konkret belegte
Move-Engpass war **drawGroupOutlines**: Die gestrichelten Umrisse liefen pro Frame
durch alle 92 ausgewählten Textkonturen, transformierten jeden Punkt erneut und
bauten jeden Canvas-Pfad neu auf. Das temporäre Auslassen nur dieser Umrisse
senkte das Overlay von 221,95 auf 0,25 ms/Frame.

**drawLayerFills war in diesem Move-Test nicht der Hauptverursacher:** Ein
Kontrolllauf mit während der Geste ausgelassener Füllung blieb bei 231,60
ms/Frame; die anschließende Abschnittsmessung wies 221,20 ms der Auswahl zu.
Die Füllung bleibt dennoch ein szenengroßer CPU-Pfad und muss sich separat am
125k-Punkte-DXF beweisen. Dieses ADR trennt deshalb den **belegten
Interaktionsengpass** (Gruppenumrisse) vom **noch zu verifizierenden
Füll-Stresstest** (DXF).

Zusätzlich wurde der Render-Takt korrigiert: Pointer-Events dürfen keinen
synchronen Full-Redraw je Event auslösen. Auch während Gesten wird höchstens
einmal pro Animation-Frame der jeweils neueste Pointer-Zustand gerendert.

### Der Offset-Fix (Grundlage, bereits umgesetzt)

Vorbedingung dieser Messung war ein Fix am WebGL-Pfad selbst: Beim Move wurde
zuvor **pro Frame die ganze Szene** neu gebaut und zur GPU hochgeladen (~16
ms/Frame bei 92 Konturen). Move ist aber eine **reine Verschiebung** — der Batch
muss nicht neu gebaut werden. Eine Offset-Uniform (`u_offset`) im Vertex-Shader
verschiebt den **einmal** hochgeladenen Batch; der Rebuild pro Frame entfällt
(→ 0,00 ms). Dieser Fix ist der erste Baustein des hier beschlossenen Pfades,
kein Wegwerf-Patch.

### Warum das kein neues Grundsatzproblem ist

ADR 0008 (2026-07-10) hat den Grundsatz bereits festgelegt: **jede** Ansicht
rendert auf der GPU, „CPU-Canvas-2D ist als Zeichenmittel nicht mehr erlaubt",
und ausdrücklich: „**Design-Canvas wird ebenfalls umgestellt … das Ziel ist, dass
keine Ansicht mehr auf CPU zeichnet.**" Der Design-Canvas ist bei einem **Hybrid**
stehengeblieben (Konturen auf GPU umgesetzt, Füllung/Auswahl/Knoten weiter 2D).

Dieses ADR ist damit **keine neue Grundsatzentscheidung**, sondern die
**Vollendung von ADR 0008 für den Design-Canvas** — plus die dabei neu zu
treffenden, konkreten Entscheidungen (wo die Füllung trianguliert wird, was
bewusst 2D bleibt). Es bekommt eine eigene Nummer, weil diese Detailentscheidungen
eigenständig und referenzierbar sind, nicht nur eine Fußnote zu 0008.

### Wie es dazu kam (dokumentierte Lehre, in der Linie von ADR 0008)

ADR 0008 hielt fest: Symptome patchen statt die Ursache messen kostet Tage. Genau
das drohte hier erneut — es wurde zunächst am WebGL-Pfad und dann am 2D-Overlay
Fix um Fix gebaut („diese Bastelei nervt", so der Nutzer wörtlich). Erst die
Messung machte die Struktur sichtbar: **zwei Renderpfade sind die Wurzel**, nicht
die einzelne langsame Funktion. Die Lehre bleibt dieselbe wie in 0008 — und der
Nutzer hat zu Recht darauf bestanden, dass der **eine saubere Pfad** jetzt
kommt, statt weiter zu flicken.

## Entscheidung

**Der Design-Canvas rendert Szeneninhalt ausschließlich über WebGL.** Der
2D-Canvas-Kontext wird für **szenengroßen** Inhalt (Konturen, Füllung,
Auswahl-Umriss, Knoten) **nicht mehr benutzt** — dieser läuft über den
bestehenden gemeinsamen WebGL-Renderer (ADR 0008 §1).

### 1. Füllung mit Löchern auf der GPU — Stencil zuerst, Triangulierung als belegter Fallback

Die gefüllte Fläche gefüllter Layer (heute `ctx.fill("evenodd")`) wird auf der GPU
gezeichnet. Der DXF-Belastungsfall (1.808 Ringe auf einem Layer) hat gezeigt, dass
der teuerste, fragilste Teil die **Even-Odd-Ring-Zuordnung** ist. Deshalb wird
**datenbasiert** vorgegangen, nicht vorab dogmatisch festgelegt:

**Plan A — Stencil-Buffer (GPU wendet Even-Odd selbst an).** Zwei Pässe je
Füll-Layer: erst die Parität beim Durchlaufen jeder Kontur im Stencil-Buffer per
Bit-Invert (INVERT) umschalten, dann die Bounding-Box-Fläche dort füllen, wo das
Paritätsbit gesetzt ist. Das vermeidet die Sättigungs-/Overflow-Probleme eines
zählenden INCR-Ansatzes. Das ist
**exakt dieselbe Regel wie das heutige `ctx.fill("evenodd")`**, nur auf der GPU —
**ohne Triangulierung, ohne Ring-Analyse, ohne n²-/Korrektheitsrisiko, ohne neue
Core-Dependency und ohne 125k-Punkte-IPC** (die Konturen liegen ohnehin schon als
Batch auf der GPU). Preis: ein zweiter GL-Pass pro Füll-Layer — bei einer Handvoll
Layern vernachlässigbar. Robust genau für den DXF-Fall.

**Plan B — Triangulierung im Core** (nur falls Stencil an einer echten Grenze
scheitert, z. B. Antialiasing-Kanten der Füllfläche). Dann ist die Triangulierung
**Geometrie und gehört in den Core** (CLAUDE.md Regel 1, nicht ins Frontend —
ThorBurn-Fehler „Canvas-Fachlogik doppelt"): `earcutr` + Even-Odd-Ring-Analyse mit
BBox-Vorfilter (§Effizienz), Core liefert Dreiecke pro Füll-Layer, Frontend lädt
sie nur hoch. Mehr Code und mehr Risiko — daher **erst bei belegtem Bedarf**.

**Entscheidungskriterium:** Plan A am echten `SW_acteken.dxf` verifizieren (Füllung
korrekt inkl. Löcher, flüssig). Nur wenn dort eine harte Grenze auftritt, Plan B
nachziehen. Beide zeichnen mit Farbe pro Vertex/Fläche über `u_mvp`/`u_offset` —
**kein neuer Shader** für die Fläche selbst (Stencil braucht nur zusätzliche
GL-State-Aufrufe, kein zweites Programm).

**Kernbefund zur Struktur:** Buchstaben-Löcher entstehen nicht aus einer Shape mit
Ring-Struktur, sondern daraus, dass **alle Shapes eines Füll-Layers gemeinsam**
mit Even-Odd gefüllt werden (jeder Buchstabe = Außenring als eigene Shape, jedes
Loch = Innenring als eigene Shape). Der Core muss also **pro Füll-Layer alle
Konturen gemeinsam** mit einer **Even-Odd-fähigen** Triangulierung verarbeiten,
nicht Shape für Shape. Umsetzung: das dependency-arme Rust-Crate `earcutr`
(Port des Standard-earcut, offline-tauglich) mit vorgelagerter
Ring-Verschachtelungs-Analyse (gerade Tiefe = Solid, ungerade = Loch).

**Effizienz-Anforderung (wegen des DXF-Belastungsfalls):** Die Ring-Analyse muss
mit **1.808 Ringen auf einem Layer** umgehen, ohne beim Laden zu hängen. Die
naive Paarbildung (jeder Ring gegen jeden, Point-in-Polygon über echte Punkte)
ist ~n² über die Ringe **und** über die Punkte — das würde genau die Trägheit
zurückbringen, die wir entfernen. Vorschrift: **Bounding-Box-Vorfilter** (Ringe
nur dort auf Containment testen, wo sich die BBoxen überlappen; Containment über
*einen* Punkt gegen den Kandidaten-Ring). Die Triangulierung selbst ist
**einmalig** (bei Scene-/Import-Änderung, gecacht), nicht pro Frame — aber ein
mehrsekündiges „Hängen beim Laden" ist genauso inakzeptabel wie Frame-Ruckeln
und wird mit dem echten DXF gemessen (Ziel: deutlich unter der Import-Zeit von
254 ms, jedenfalls kein spürbarer Hänger).

### 2. Auswahl-Umriss und Knoten auf die GPU

`drawGroupOutlines` (gestrichelter Umriss jeder selektierten Kontur) und
`drawNodes` (Knoten je Punkt) skalieren ebenfalls mit der Shape-/Punktzahl und
gehören daher auf die GPU — als Linien-/Punkt-Batches (der Renderer kann `points`
bereits). Beim Move folgen sie per `u_offset` demselben Prinzip wie die Konturen.

### 3. Was bewusst im 2D-Canvas bleibt (Klasse B)

ADR 0008 zielt auf „was pro Frame durch zehntausende Segmente geht", nicht auf ein
Lineal. Elemente, deren Kosten **nicht mit der Shape-Zahl skalieren**, bleiben im
2D-Kontext — sie umzubauen brächte keinen Performancegewinn, nur Aufwand und Risiko
(z. B. GPU-Text für die mm-Lineale):

- **Lineale** (mm-Ziffern + Ticks, `fillText`),
- Mess-Overlay, Bridge-/Bézier-Entwurf, Fillet-Marker, Laser-Marker,
- die gemeinsame Auswahl-**Box** (nur 4 Ecken, unabhängig von der Shape-Zahl).

Der 2D-Canvas verschwindet also **nicht als Element**, aber **kein
szenengroßer Inhalt** läuft mehr darüber. Das erfüllt die ADR-0008-Invariante.

### 4. WebGL1 bleibt

Kein Wechsel auf WebGL2 oder WebGPU. WebGPU ist in der Ziel-WebView (WebKitGTK
2.52 unter Wayland) **nicht verfügbar**. WebGL1 trägt die Last laut
ADR-0008-Messung (1 Mio Segmente < 1 ms) mühelos und beherrscht sowohl den
Stencil-Buffer (Plan A) als auch Triangle-Füllung (Plan B). Keine neue
Render-Lib (ADR 0008 §1).

## Invarianten

1. **Kein szenengroßer Inhalt auf CPU-Canvas-2D.** Konturen, Füllung,
   Auswahl-Umriss und Knoten des Design-Canvas rendern über WebGL. Klasse-B-
   Ausnahmen (§3) sind abschließend benannt; neue szenenabhängige Zeichnung
   darf nicht auf `getContext("2d")` zurückfallen.
2. **Falls trianguliert wird (Plan B), ist das Core-Fachlogik** (CLAUDE.md Regel
   1), testbar, UI-frei — nicht im Frontend. (Plan A/Stencil braucht keine
   Triangulierung; die Even-Odd-Regel macht die GPU.)
3. **Eine Wahrheit:** Die gezeichnete Füllfläche wird aus denselben Konturen
   abgeleitet, die auch Kontur/Job speisen — egal ob per Stencil oder Dreiecke.
4. **Live-Gesten über Uniform, nicht über Rebuild.** Move verschiebt gecachte
   Batches per `u_offset`; nur Scale/Rotate bauen den selektierten Teil pro Frame
   neu (nicht rein translatorisch).
5. **Ein Render-Takt.** Pointer- und Svelte-Ereignisse aktualisieren nur den
   neuesten Zustand; tatsächlich gezeichnet wird höchstens einmal pro
   Animation-Frame. Kein synchroner Full-Redraw je Pointer-Event.

## Konsequenzen

- **Plan A (Stencil)**: Renderer (`gl/renderer.ts`) bekommt einen Füll-Pfad, der
  mit den vorhandenen Konturen-Batches je Layer das Paritätsbit im Stencil-
  Buffer invertiert und die Fläche Even-Odd füllt. Kein neues Core-Modul, keine
  neue Dependency, kein neuer IPC — die Konturen liegen schon als Batch vor.
  Alpha-Blending ist vorhanden.
- **Nur bei Plan B (Fallback)**: Core-Modul `fill_tessellate` (earcutr +
  BBox-Ring-Analyse) + Tests (Glyphen „O"/„B"/„g", Rechteck, verschachtelte Ringe,
  **das DXF**); neuer Tauri-Command (Dreiecke + Farbe pro Layer); Triangle-Modus
  im Renderer. Dependency `earcutr` erst dann.
- `Canvas.svelte`: `drawLayerFills`/`drawGroupOutlines`/`drawNodes` werden durch
  GPU-Batches ersetzt; die 2D-Zeichner für Szeneninhalt entfallen.
- **Belegter erster Umsetzungsschritt:** Die Gruppenumrisse auf einen gecachten
  GPU-Linienbatch umstellen. Bis dahin werden die teuren Einzelkonturen während
  Move/Scale/Rotate nicht auf CPU neu aufgebaut; gemeinsame Auswahlbox und
  Griffe bleiben als Live-Rückmeldung sichtbar.
- **Restrisiko Plan A (Stencil):** Stencil-Even-Odd ist bewährt (Standard-
  Verfahren für Polygonfüllung), umgeht Ring-Zuordnung und n²-Problem komplett.
  Mögliche Grenze: **weiche Kanten/Antialiasing** der Füllfläche (der Stencil
  füllt pixelscharf; die halbtransparente Fläche sollte trotzdem sauber wirken).
  Genau das wird am `SW_acteken.dxf` verifiziert. Tritt es auf → Plan B.
- **Plan B (Triangulierung) als belegter Fallback**, kein Notnagel: sauberere
  Kanten, aber muss die 1.808-Ringe-Even-Odd korrekt **und** schnell lösen (BBox-
  Vorfilter, §1). Wird nur gebaut, wenn Plan A an einer echten Grenze scheitert.
- Kontextverlust-Behandlung: Füll-Batch/-State in denselben Rebuild-/Free-Pfad wie
  der Konturen-Batch einhängen (vorhanden).

## Nicht Teil dieser Entscheidung

- **Laser-Preview / Laser-Ansicht** — bereits über die WebGL-Schicht (ADR 0008);
  hier geht es nur um den Design-Canvas.
- **GPU-Text für die Lineale** — bewusst nicht (Klasse B, §3).
- **WebGL2 / WebGPU / wgpu-in-Rust** — spätere Optionen; die Render-API kapselt
  einen späteren Wechsel, aber jetzt bleibt es WebGL1 (§4).

## Umsetzungsprotokoll

Dieser Abschnitt wird nach **jedem stabilen Teilschritt** aktualisiert. Er ist
die Wiederaufnahmestelle für einen neuen Arbeitslauf: Der erste offene Punkt
unter „Als Nächstes“ ist der verbindliche Einstieg; Messwerte und bewusst
temporäre Zustände dürfen nicht nur im Chat stehen.

### Stand 2026-07-11 — Ausgangslage vor der ADR-Umsetzung

**Erledigt**

- WebGL-Konturen liegen in gecachten Batches; Move nutzt eine Offset-Uniform.
- Render-Scheduling ist auch während Pointer-Gesten auf höchstens einen
  Animation-Frame begrenzt.
- Engpass isoliert: Auswahl/Gruppenumrisse 221,20 ms von 221,95 ms Overlay-Zeit.
- Temporärer Schutz: Gruppenumrisse sowie CPU-Füllung werden während
  Move/Scale/Rotate nicht gezeichnet; dadurch 0,25 ms Overlay-Zeit.

**Bewusst temporär**

- Die Gruppenumrisse verschwinden während Transform-Gesten, statt der Auswahl
  auf der GPU zu folgen.
- Die Füllung verschwindet während Transform-Gesten; ihr GPU-Pfad fehlt.
- Temporäre Performance-Protokollierung ist im Frontend noch aktiv.

**Als Nächstes**

1. Gruppenumrisse als gecachten GPU-Linienbatch rendern und bei Move über
   dieselbe Offset-Uniform wie die Kontur verschieben.
2. Scale/Rotate-Verhalten und visuelle Strichelungs-Parität festlegen und
   messen; keine Rückkehr zum punktzahlabhängigen CPU-Pfad.
3. Danach Knoten als GPU-Punkt-/Linienbatch umstellen.
4. Anschließend Stencil-Füllung (Plan A) implementieren und am großen DXF
   verifizieren.

### Stand 2026-07-11 — Teilschritt 1: Gruppenumrisse auf GPU

**Implementiert**

- Neuer reiner Batch-Bauer für Gruppenumrisse im gemeinsamen Design-Renderpfad.
- Gruppenumrisse liegen in Ruhe als gecachter GPU-Linienbatch vor.
- Move verschiebt diesen Batch ausschließlich über die vorhandene
  Offset-Uniform; kein Punkt-Rebuild und kein Upload pro Frame.
- Scale/Rotate bauen vorerst ausschließlich den selektierten Umriss-Batch neu.
- Der bisherige CPU-Zeichner für punktzahlabhängige Gruppenumrisse ist entfernt;
  die gemeinsame Auswahlbox und Griffe bleiben als konstantes 2D-Overlay.

**Bewusste Zwischenstufe**

- GPU-Gruppenumrisse sind zunächst durchgezogen statt 5/3 px gestrichelt.
  Bildschirmkonstante Strichelung benötigt Distanzdaten beziehungsweise einen
  eigenen Shaderpfad und darf nicht durch CPU-Segmentierung pro Frame erkauft
  werden.

**Validierung**

- `npm run check`: 0 Fehler, 0 Warnungen.
- `npm run build`: Produktions-Build erfolgreich.
- `git diff --check`: sauber.
- Laufzeittest mit 92 ausgewählten Textkonturen:
  - WebGL-Kontur bei Move: 0,00–0,05 ms/Frame.
  - 2D-Overlay bei Move: 0,50–0,70 ms/Frame.
  - Auswahlanteil: 0,05–0,15 ms/Frame statt zuvor 221,20 ms.
- Laufzeittest mit 1.808 ausgewählten DXF-Konturen:
  - WebGL-Kontur im Move-Fastpath: 0,00 ms/Frame.
  - Ein transformierender Rebuild-Pfad wurde mit 8,60 ms/Frame beobachtet;
    Move selbst bleibt per Offset-Uniform upload-frei.
  - 2D-Overlay während der Geste ohne CPU-Füllung: 1,55 ms/Frame.
- Gruppenumrisse folgen der Auswahl über den GPU-Pfad; der gemessene
  221-ms-CPU-Engpass ist beseitigt.

**Als Nächstes**

1. Wegen des nachfolgend belegten 350-ms-Füllpfads Stencil-Füllung vorziehen.
2. Danach Strichelungs-Parität oder – falls funktional bewusst entbehrlich –
   die dauerhafte durchgezogene GPU-Markierung im ADR festlegen.

### Stand 2026-07-11 — Teilschritt 2: Füll-Engpass am DXF belegt

**Messung**

- 1.808 Konturen: CPU-Füllung 10,20 ms/Frame in einem kleineren/anderen
  Ansichtsfall und **350,05 ms/Frame** im belastenden DXF-Redraw.
- Gesamt-Overlay im Belastungsfall: 355,50 ms/Frame; davon entfallen rund
  98,5 Prozent auf **drawLayerFills**.
- Wird die Füllung während der Geste ausgelassen, fällt dasselbe Overlay auf
  0,75–1,55 ms/Frame.
- Damit sind jetzt beide szenengroßen CPU-Pfade separat belegt:
  Gruppenumrisse waren der Text-Move-Engpass; Füllung ist der DXF-Engpass.

**Entscheidung für die Reihenfolge**

- Plan A/Stenciling wird vor dem Node-Batch umgesetzt. Der Node-Pfad ist nur im
  Node-Werkzeug aktiv; die Füllung belastet dagegen jeden relevanten Redraw
  eines gefüllten großen Imports.

**Als Nächstes**

1. WebGL-Kontext mit Stencil-Buffer anfordern und dessen Verfügbarkeit prüfen.
2. Konturen je gefülltem Layer als eigene GPU-Batches bereitstellen.
3. Paritätspass mit INVERT und anschließenden Flächenpass implementieren.
4. Rechteck, Textlöcher und das große DXF visuell sowie per Frame-Messung prüfen.

### Stand 2026-07-11 — Teilschritt 3: erster Stencil-Füllpfad

**Implementiert**

- WebGL-Kontext fordert einen Stencil-Buffer explizit an.
- Geschlossene Konturen werden pro Fill-/Raster-Layer einmalig in einen
  gemeinsamen Positionsbuffer gepackt; je Kontur bleiben nur Start und Anzahl
  der Fan-Vertices als Draw-Bereich erhalten.
- Stencil-Pass invertiert Bit 0 je Kontur-Fan; ein anschließendes
  Bounding-Quad zeichnet die Layerfarbe nur bei gesetztem Paritätsbit.
- Der bisherige CPU-Pfad **drawLayerFills** und seine Punktkopien/
  Bildschirmtransformationen sind vollständig entfernt.
- Füll-Batches werden zusammen mit der Szenengeometrie aufgebaut und
  freigegeben; Pan/Zoom erzeugen keinen neuen Punkt-Batch.

**Bewusste Zwischenstufe**

- Während Move/Scale/Rotate bleibt die Füllung noch ausgeblendet. Live-Transform
  der Stencil-Konturen folgt nach bestätigter geometrischer Korrektheit.
- Der Kontur-Fan-Paritätsansatz muss an konkaven Konturen, Buchstabenlöchern und
  dem realen DXF visuell bestätigt werden; bis dahin gilt Plan A nicht als
  abgeschlossen.

**Statische Validierung**

- `npm run check`: 0 Fehler, 0 Warnungen.
- `npm run build`: Produktions-Build erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Rechteck und einzelne konkave Kontur prüfen.
2. Text „O/B/g“ auf korrekte Löcher und gemeinsame Even-Odd-Parität prüfen.
3. Großes DXF auf korrekte Fläche, Löcher und Redraw-Zeit prüfen.
4. Bei korrekter Geometrie Live-Move per Offset ergänzen; bei Fan-Artefakten
   Plan A korrigieren, bevor weitere Renderer-Funktionen folgen.

### Stand 2026-07-11 — Teilschritt 3a: CPU-Füllpfad messtechnisch beseitigt

**Bestätigt nach Vite-Hot-Reload**

- Szenen mit 3–5 Shapes: 2D-Overlay 0,40–0,80 ms/Frame.
- Gemessener Füllanteil im 2D-Overlay: 0,00–0,05 ms/Frame.
- WebGL-Kontur während der beobachteten Gesten: 0,00–0,85 ms/Frame.
- Der frühere CPU-Füllpfad mit bis zu 350,05 ms/Frame erscheint nach dem
  Stencil-Umbau nicht mehr in der Abschnittsmessung.

**Noch nicht bestätigt**

- Die neuen Logs enthalten nach dem Hot-Reload keinen erneuten Lauf mit 1.808
  DXF-Konturen; die vorherigen 1.808-/350-ms-Zeilen stammen aus dem alten
  CPU-Füllpfad vor dem Reload.
- Visuelle Korrektheit von konkaven Konturen, Buchstabenlöchern und DXF-Flächen
  wurde noch nicht berichtet.
- Plan A bleibt deshalb **in Arbeit**, obwohl die CPU-Zeit bereits beseitigt ist.

**Wiederaufnahmestelle**

1. Das große DXF erneut laden und sowohl Darstellung als auch neue Messwerte
   melden.
2. Erst bei korrekter DXF-Parität Live-Transformation der Füllung implementieren.

### Stand 2026-07-11 — Teilschritt 3b: Glyphen-Parität visuell bestätigt

**Bestanden**

- Gemischter Text mit a/A/o/O/B/b/P/Q wird korrekt gefüllt.
- Innenräume von A, O, B, P und Q bleiben frei.
- Mehrere Außen- und Innenkonturen auf demselben Fill-Layer verhalten sich
  gemeinsam nach Even-Odd.
- Keine sichtbaren Fan-Dreiecke oder über die Glyphenkontur hinausragenden
  Füllartefakte im geprüften Text.

**Noch offen**

- Reales DXF mit 1.808 Konturen und 125.305 Punkten visuell prüfen.
- Neue Framewerte dieses DXF nach dem Stencil-Umbau erfassen.
- Erst danach Plan A als geometrisch belastbar markieren und Live-Move der
  Füllung ergänzen.

### Stand 2026-07-11 — Teilschritt 3c: konkave Konturen bestätigt

**Bestanden**

- Konkaver fünfzackiger Stern wird vollständig innerhalb seiner Kontur gefüllt.
- Herzform mit tiefer oberer Einbuchtung zeigt keine Fan-Dreiecke oder
  überstehenden Flächen.
- Rechteck, Kreis, Dreieck, Raute und konvexe Polygone werden korrekt gefüllt.
- Konturlinie und Stencil-Fläche liegen in den geprüften Formen deckungsgleich.

**Verbleibender Abschluss für Plan A**

1. Großes DXF mit 1.808 Konturen erneut prüfen.
2. Darstellung und Messwerte dokumentieren.
3. Danach Live-Transformation der Stencil-Füllung umsetzen.

### Stand 2026-07-11 — Teilschritt 3d: realer DXF-Stresstest bestanden

**Geometrie**

- Reales DXF mit 1.808 Konturen und 125.305 Punkten wird korrekt gefüllt.
- Komplexe konkave Außenflächen, zahlreiche Löcher und sehr feine Aussparungen
  sind im geprüften Ausschnitt korrekt.
- Keine sichtbaren Fan-Dreiecke oder großflächigen Paritätsartefakte.
- Plan A ist damit für Rechteck, konkave Formen, Glyphenlöcher und den realen
  Belastungsfall geometrisch bestätigt.

**Messung nach Stencil-Umbau**

- CPU-Füllanteil: 0,00 ms/Frame.
- 2D-Overlay beim großen DXF: ungefähr 2,30–2,70 ms/Frame; davon rund
  1,95–2,15 ms für die weiterhin über alle Shapes laufende Bildprüfung.
- WebGL-Kontur im Move-Fastpath: 0,00–0,05 ms/Frame.
- Ein transformierender Rebuild wurde mit 46,45 ms/Frame gemessen und bleibt
  ein eigener Scale-/Rotate-Optimierungspunkt; Move ist davon nicht betroffen.

### Stand 2026-07-11 — Teilschritt 4: Stencil-Füllung folgt Live-Move

**Implementiert**

- Ist ein Fill-Layer vollständig ausgewählt, verschiebt Move Konturen,
  Stencil-Fans und Bounding-Quad gemeinsam über dieselbe Offset-Uniform.
- Kein Punkt-Rebuild und kein GPU-Upload pro Move-Frame.
- Der Auswahlstatus der Füll-Batches wird bei Auswahlwechsel aktualisiert.

**Bewusste Grenze**

- Bei nur teilweise ausgewähltem Fill-Layer bleibt dessen Füllung während Move
  noch ausgeblendet; sonst würden ausgewählte und statische Ringe gemeinsam
  falsch verschoben.
- Scale/Rotate blenden die Füllung weiterhin aus, bis transformierte
  Stencil-Batches ohne den gemessenen 46-ms-Rebuild bereitstehen.

**Statische Validierung**

- `npm run check`: 0 Fehler, 0 Warnungen.
- `npm run build`: erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Live-Move mit vollständig ausgewähltem Text und DXF visuell/messbar prüfen.
2. Fill-Layer in statische und ausgewählte Fans aufteilen, damit partielle
   Auswahl korrekt live gefüllt bleibt.
3. Scale/Rotate-Transform für Stencil-Fans ergänzen und messen.

### Stand 2026-07-11 — Teilschritt 4a: Pan- und Auswahl-Latenz korrigiert

**Neuer Messbefund**

- DXF-Move bleibt nach dem ersten Frame bei 0,00–0,05 ms Konturzeit und ist
  subjektiv flüssig.
- Pan blieb dagegen stark träge; der Stencil-Pass erzeugte pro Redraw 1.808
  einzelne TRIANGLE_FAN-Draw-Calls.
- Auswahlwechsel lud zusätzlich den vollständigen 125k-Punkte-Füllbuffer erneut
  hoch, obwohl sich nur der Auswahlstatus änderte.
- Einzelne 15,15-/16,15-/31,00-ms-Werte zeigen den teuren Aufbau-/Transformweg;
  sie gehören nicht zum stabilen Move-Fastpath.

**Implementiert**

- Jeder Kontur-Fan wird beim einmaligen Szenenaufbau zu Dreiecken expandiert.
  Alle Konturen eines Layers laufen anschließend in **einem**
  drawArrays(TRIANGLES)-Paritätspass statt in 1.808 einzelnen Draw-Calls.
- Füllbatches tragen ihre Layer-ID; Auswahlwechsel aktualisieren nur noch das
  allSelected-Metadatum.
- Kein erneuter Upload der 125k Füllpunkte allein durch Anklicken/Selektieren.

**Trade-off**

- Der einmalige GPU-Füllbuffer ist durch die Dreiecksexpansion größer
  (ungefähr drei Vertices je Fan-Dreieck). Das ist bewusst: einmalige
  Speicherbelegung ersetzt tausende Draw-Calls bei jedem Pan-/Zoom-Frame.

**Statische Validierung**

- `npm run check`: 0 Fehler, 0 Warnungen.
- `npm run build`: erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Großes DXF erneut pannen und Auswahl-Latenz prüfen.
2. Füllkorrektheit nach Fan-zu-Triangle-Expansion gegen den vorherigen
   DXF-Screenshot vergleichen.
3. Neue Messwerte dokumentieren, dann partielle Fill-Layer-Auswahl umsetzen.

### Stand 2026-07-11 — Messmethodik für kurze Gesten korrigiert

**Problem der bisherigen Messung**

- Die Ausgabe erfolgte erst nach jeweils 20 Frames.
- Ein einzelner Klick und kurze Bewegungen erzeugten keine Ausgabe.
- Bei langen Gesten verdünnten viele schnelle Fastpath-Frames die teure
  Startlatenz; der Mittelwert beschrieb deshalb nicht das wahrgenommene Hängen.

**Implementiert**

- Jede Pointer-Geste beginnt eine eigene Messperiode.
- Beim Loslassen werden auch für Ein-Frame-Gesten ausgegeben:
  Frameanzahl, Gesamtdauer sowie erster, maximaler und mittlerer Wert für
  vollständiges WebGL-Rendering und den gesamten Canvas-Frame.
- Der Auswahl-Command wird separat als **select_at Core + Scene** gemessen.
- Damit lassen sich Core-Hit-Test, erster GPU-Batch-Aufbau und stabiler
  Fastpath auseinanderhalten.

**Statische Validierung**

- `npm run check`: 0 Fehler, 0 Warnungen.
- `npm run build`: erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Einen einfachen Klick auf das große DXF ausführen.
2. Eine sehr kurze Pan-Geste und eine längere Pan-Geste ausführen.
3. Jeweils die neue **select_at**- beziehungsweise **gesture pan**-Zeile
   vergleichen; erst danach die tatsächlich verantwortliche Stufe optimieren.

### Stand 2026-07-11 — Teilschritt 4b: Klick- und erster Move-Frame behoben

**Messung mit gestenbasierter Methodik**

- Auswahl-Command inklusive voller Scene-Rückgabe: 1.279–2.285 ms.
- Pan-Rendering selbst: Geo first/max/avg 1,00/1,00/0,67 ms; gesamter
  Renderframe 6,00–10,00 ms. Der Renderer ist damit nicht die Pan-Bremse.
- Move bei vollständig ausgewähltem DXF: erster Frame 339–350 ms, danach
  schneller Fastpath. Ursache war erneuter Aufbau der bereits vorhandenen
  vollständigen Konturgeometrie.

**Ursachen**

- **select_at** und **select_rect** serialisierten nach reiner Auswahländerung
  die komplette 125k-Punkte-Szene über Tauri zurück zum Frontend.
- Bei Vollauswahl wurde ein leerer statischer Teil wiederholt gesucht und der
  vollständige Szenenbatch zusätzlich als movedBatch aufgebaut.

**Implementiert**

- Auswahl-Commands liefern nur noch **selected** und **selection_bbox**.
  Das Frontend führt diese kleine kanonische Auswahlantwort in die vorhandene
  Scene ein; Layers und Shapes werden nicht erneut übertragen.
- Ist die gesamte Szene ausgewählt, verwendet Move den bereits gecachten
  shapeBatch direkt mit der Offset-Uniform.
- Kein Aufbau eines leeren statischen Batches und kein zweiter 125k-Punkte-
  Konturbatch für den ersten Move-Frame.

**Validierung**

- Frontend `npm run check`: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- 198 Core-Tests bestanden.
- Tauri-Backend `cargo check --manifest-path luxifer/frontend/src-tauri/Cargo.toml`
  erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Klicklatenz und ersten Move-Frame am großen DXF erneut messen.
2. Falls **select_at** trotz schlanker Antwort langsam bleibt, Core-Hit-Test
   separat instrumentieren und mit BBox-/Raumindex beschleunigen.
3. Danach partielle Fill-Layer-Auswahl und Scale/Rotate fortsetzen.

### Stand 2026-07-11 — Teilschritt 4c: Core-Hit-Test vorgefiltert

**Nachmessung**

- Schlanke Auswahlantwort allein reichte nicht: **select_at** benötigte beim
  großen DXF weiterhin 1.599 ms.
- Renderpfade sind dagegen bestätigt schnell:
  - Move Geo first/max/avg 5,00/5,00/1,34 ms bei 38 Frames.
  - Move Total first/max/avg 14,00/14,00/4,32 ms.
  - Pan Geo first/max/avg 1,00/1,00/0,67 ms.
  - Pan Total first/max/avg 6,00/10,00/7,33 ms.
- Der verbleibende Klick-Hänger liegt damit im exakten Core-Hit-Test.

**Implementiert**

- Jede Shape wird vor dem exakten Punkt-Segment-Test gegen ihre um die
  Klicktoleranz erweiterte Bounding-Box geprüft.
- Nur Konturen, deren BBox den Klick überhaupt enthalten kann, führen die
  teure Distanzprüfung über ihre Segmente aus.
- Semantik für sichtbare/gesperrte Layer und oberste Shape bleibt unverändert.

**Validierung**

- `cargo fmt --check`: sauber.
- 198 Core-Tests bestanden.
- Tauri-Backend-Check erfolgreich.
- Frontend `npm run check`: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. **select_at** am großen DXF erneut messen.
2. Falls die BBox-Berechnung selbst noch zu teuer ist, persistente Shape-BBoxen
   beziehungsweise einen räumlichen Index als nächsten Core-Schritt bewerten.

### Stand 2026-07-11 — Teilschritt 4d: Auswahl invalidiert Geometrie nicht mehr

**Nachmessung**

- BBox-Vorfilter allein senkte **select_at Core + Scene** nur von 1.599 auf
  1.361 ms. Der exakte Core-Hit-Test war damit nicht die ganze Restlatenz.

**Zusätzliche Ursache**

- Das Frontend mischte die kleine Auswahlantwort über ein neues Scene-Objekt ein.
- Svelte bewertete dadurch den Geometrie-Effect neu, obwohl Shapes und Layers
  referenziell unverändert waren.
- Kontur-, Füll- und Auswahlbatches wurden nach dem Klick unnötig neu aufgebaut;
  diese reaktive Folgekosten waren in der um den Callback gelegten
  **Core + Scene**-Messung enthalten.

**Implementiert**

- Auswahlantwort mutiert ausschließlich **scene.selected** und
  **scene.selection_bbox**; die Scene-Identität und Geometriereferenzen bleiben
  erhalten.
- Der Geometrie-Effect besitzt zusätzlich eine Referenz-/Dirty-Grenze für
  Shapes und Layers.
- Geometrieänderungen während einer Geste bleiben dirty und werden nach
  Gestenende genau einmal aufgebaut; reine Auswahländerungen lösen keinen
  Geometrie-Reupload aus.

**Validierung**

- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- 198 Core-Tests bestanden.
- Tauri-Backend-Check erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Klicklatenz erneut messen; erwartet wird jetzt die tatsächliche Core-Zeit
   ohne nachgeschalteten 125k-Punkte-Geometrieaufbau.
2. Nur falls diese noch relevant hoch ist, persistente Core-Bounds/Raumindex
   weiterverfolgen.

### Stand 2026-07-11 — Teilschritt 4e: Core-eigener Bounds-Cache

**Implementiert**

- AppState hält einen abgeleiteten, nicht persistierten Bounds-Cache mit einer
  BBox je Shape.
- Cache wird lazy bei der ersten Hit-Test-/Auswahlbox-Abfrage aufgebaut.
- Hit-Test-BBox-Vorfilter und selection_bbox verwenden dieselbe gecachte
  Core-Geometrie.
- Cache wird bei push_undo sowie Undo-/Redo-Restore invalidiert; damit ist er
  nach Import, Move, Scale, Rotate und Geometrieoperationen beim nächsten Lesen
  automatisch neu aufgebaut.
- Projektformat, Scene-DTO und Frontend enthalten keine duplizierte
  Hit-Test-Geometrie.

**Erwarteter Effekt**

- Der erste Hit-Test nach einer Geometrieänderung baut Bounds einmalig aus den
  Punkten auf.
- Weitere Klicks und Auswahlbox-Abfragen lesen Bounds in O(1) je Shape; nur
  BBox-Kandidaten führen den exakten Segment-Hit-Test aus.

**Validierung**

- Rust-Formatierung sauber.
- 198 Core-Tests bestanden.
- Tauri-Backend-Check erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Ersten und zweiten Klick nach DXF-Import getrennt messen.
2. Falls nur der erste Klick noch auffällt, Bounds bereits beim Import-/Scene-
   Aufbau vorwärmen; falls beide langsam bleiben, echten Raumindex ergänzen.

### Stand 2026-07-11 — Teilschritt 4f: Core-Auswahl intern instrumentiert

**Anlass**

- Nach Bounds-Cache lag ein gemessener Auswahlaufruf weiterhin bei 548 ms.
- Renderframe blieb mit 0,5–1,0 ms Geo und 5,5–7,5 ms Gesamt unauffällig.
- Die äußere Callback-Zeit kann Core-Lock, Hit-Test, Gruppenerweiterung,
  Auswahlbox und Frontend-Folgekosten nicht unterscheiden.

**Implementiert**

- Auswahlantwort enthält temporär vier interne Core-Zeiten:
  State-Lock, hit_test, expand_selection_to_groups und selection_bbox.
- Frontend protokolliert diese als **select_at core parts**.
- Damit wird der nächste Schritt ausschließlich am tatsächlich dominanten
  Abschnitt angesetzt.

**Validierung**

- 198 Core-Tests bestanden.
- Tauri-Backend-Check erfolgreich.
- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.

**Als Nächstes**

1. Einen Auswahlklick ausführen und sowohl **select_at core parts** als auch
   äußere **select_at Core + Scene** melden.
2. Dominanten Core-Abschnitt gezielt optimieren; temporäre Detailmessung nach
   Abschluss wieder entfernen.

### Stand 2026-07-11 — Teilschritt 4g: Vollgruppen-Auswahl ohne Batch-Rebuild

**Interne Messung**

- Core ist nicht der verbleibende Engpass:
  - Lock 0,00 ms.
  - Hit-Test 0,02–0,64 ms.
  - Gruppenerweiterung 0,00–0,32 ms.
  - Auswahlbox 0,00–0,01 ms.
- Äußere Auswahlzeit schwankte dennoch zwischen 6 und 429 ms.
- Hohe Werte traten beim Auswählen der 1.808-konturigen Gesamtgruppe auf;
  Abwählen ohne Gruppenumriss-Batch lag bei 6 ms.

**Ursache**

- Der Frontend-Auswahleffekt kopierte für die blaue Gruppenmarkierung dieselben
  125k Konturpunkte erneut und lud einen separaten GPU-Positionsbuffer hoch.

**Implementiert**

- Renderer kann einen vorhandenen Positionsbuffer mit konstanter Farbe
  zeichnen, ohne dessen Farbbuffer zu kopieren.
- Ist die gesamte Szene als echte Gruppe ausgewählt, verwendet der blaue
  Gruppenumriss direkt den vorhandenen shapeBatch.
- Ruhe und Move überschreiben nur die Vertexfarbe; Move nutzt zusätzlich
  weiterhin die Offset-Uniform.
- Partielle Gruppen verwenden vorerst weiterhin ihren eigenen Auswahlbatch.

**Validierung**

- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- Tauri-Backend-Check erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Große Gruppe an- und abwählen; äußere Auswahlzeit erneut messen.
2. Bei bestätigter Verbesserung temporäre Core-Detailmessung entfernen.
3. Danach partielle Gruppenbatches beziehungsweise Nodes fortsetzen.

### Stand 2026-07-11 — Teilschritt 4h: Auswahl-Latenz abgeschlossen

**Abschlussmessung am großen DXF**

- Erste Auswahl der 1.808-konturigen Gruppe: 56 ms.
- Direktes Abwählen: 8 ms.
- Erneutes Auswählen mit warmen Caches: 16 ms.
- Core-Anteile blieben jeweils unter 1 ms.
- Renderframes: Geo 0–1 ms, Gesamt 2–5 ms.
- Ausgangswert vor den Auswahloptimierungen: 1.279–2.285 ms.

**Ergebnis**

- Auswahlantwort ist schlank.
- Reine Auswahl invalidiert keine Geometriebatches.
- Bounds sind Core-seitig gecacht.
- Vollgruppen-Markierung verwendet den vorhandenen shapeBatch.
- Klick- und Auswahl-Latenz gelten für den dokumentierten Belastungsfall als
  editor-tauglich.

**Aufräumen**

- Temporäre Core-Detailmessung (Lock/Hit/Groups/BBox) wieder entfernt.
- Gestenmessung bleibt bis zur Scale-/Rotate-Optimierung aktiv.

**Als Nächstes**

1. Partielle Fill-Layer-Auswahl für sichtbaren Live-Move aufteilen.
2. Scale/Rotate ohne Punkt-Rebuild pro Frame umsetzen.
3. Danach Nodes auf GPU und temporäre Gestenmessung entfernen.

### Stand 2026-07-11 — Teilschritt 5: Scale/Rotate über Model-Matrix

**Implementiert**

- Linien-Shader besitzt zusätzlich zur Kamera- und Offset-Uniform eine
  Model-Matrix.
- Scale wird als affine Skalierung von Start- auf Zielbox berechnet.
- Rotate wird als Rotation um den kanonischen Auswahlmittelpunkt berechnet.
- Konturen verwenden für Scale/Rotate einen einmalig gecachten Auswahlbatch;
  kein Punkt-Rebuild und kein GPU-Upload pro Frame.
- Vollständig ausgewählte Stencil-Füllungen folgen derselben Model-Matrix.
- GPU-Gruppenumrisse folgen ebenfalls derselben Matrix.
- Der bisherige JavaScript-Punkttransformer und temporäre Scale-/Rotate-Batch
  pro Frame sind entfernt.

**Weiterhin offen**

- Partielle Fill-Layer-Auswahl bleibt während Transformationen noch ohne
  Füllung, bis statische und ausgewählte Stencil-Teile getrennt vorliegen.
- Visuelle und gestenbasierte Laufzeitprüfung für Scale und Rotate steht aus.

**Statische Validierung**

- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Scale und Rotate mit Text sowie großem DXF visuell prüfen.
2. **gesture scale/rotate** first/max/avg dokumentieren.
3. Danach partielle Fill-Layer-Aufteilung und Nodes umsetzen.

### Stand 2026-07-11 — Teilschritt 5a: Model-Matrix visuell bestätigt

**Bestanden**

- Scale und Rotate verhalten sich im manuellen Test korrekt.
- Kontur, Füllung, Gruppenumriss und Auswahlbox bleiben deckungsgleich.
- Der frühere Punkt-Rebuild-/Upload-Pfad bleibt entfernt.

**Nächste technische Grenze: partielle Fill-Layer-Auswahl**

- Ein bloßer Neuaufbau zweier großer Fill-Batches bei jedem Auswahlwechsel
  würde die gerade beseitigte Auswahl-Latenz wieder einführen.
- Viele einzelne Shape-Buffer würden beim DXF erneut tausende Draw-Calls
  erzeugen und die beseitigte Pan-Latenz zurückbringen.
- Der nächste Schnitt muss deshalb pro Layer stabile Shape-Ranges in einem
  Buffer halten und statische/ausgewählte Ranges in **einem** Stencil-
  Paritätspass kombinieren.
- Der Farbpass darf nicht über getrennte Layer-Bounding-Quads erfolgen, weil
  sich statische und bewegte Ringe weiterhin gemeinsam nach Even-Odd verhalten
  müssen.

**Als Nächstes**

1. Stencil-API auf kombinierten statischen/transformierten Paritätspass
   erweitern.
2. Stabile Range-Metadaten pro Shape ergänzen, ohne Punktdaten bei
   Auswahlwechsel neu hochzuladen.
3. Partielle Move-/Scale-/Rotate-Auswahl visuell und mit großem DXF prüfen.

### Stand 2026-07-11 — Teilschritt 6: partielle Fill-Layer-Transformation

**Implementiert**

- Jeder Fill-Layer-Buffer trägt stabile Shape-Ranges (Shape-Index,
  Startvertex, Vertexzahl).
- Auswahlwechsel verändern nur die verwendeten Ranges; Punktdaten werden nicht
  kopiert oder neu hochgeladen.
- Statische Ranges laufen mit Identitätsmatrix, ausgewählte Ranges mit
  Move-Offset beziehungsweise Scale-/Rotate-Model-Matrix.
- Beide Teile schreiben in **denselben** Stencil-Paritätspass; Even-Odd bleibt
  damit über den gesamten Layer korrekt.
- Der Farbpass zeichnet dieselben Dreiecke ein zweites Mal und setzt getroffene
  Stencilpixel sofort auf 0. Dadurch wird jedes Innenpixel genau einmal
  geblendet und ein transformabhängiges Bounding-Quad entfällt.
- Benachbarte Ranges mit gleichem Transformstatus werden zu Draw-Runs
  zusammengefasst. Ruhe und Vollauswahl bleiben dadurch bei einem Run statt
  wieder auf tausende Draw-Calls zurückzufallen.

**Statische Validierung**

- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Auf einem Fill-Layer nur eine von mehreren Formen auswählen.
2. Move, Scale und Rotate prüfen: ausgewählte Füllung muss folgen, statische
   Füllung stehen bleiben, gemeinsame Löcher müssen korrekt bleiben.
3. Danach großen DXF vollständig bewegen/pannen und Run-Regression ausschließen.

### Stand 2026-07-11 — Teilschritt 6a: WebGL-Farbargument robust

**Fehler**

- Der neue Stencil-Range-Pfad löste wiederholt
  „Spread syntax requires iterable“ in drawStencilFillParts aus.
- Ursache war das Entpacken eines zur Laufzeit nicht garantiert iterierbaren
  Farb-Tuples in WebGLs vertexAttrib4f; HMR/proxifizierte Werte dürfen an dieser
  Low-Level-Grenze nicht auf Iteratorsemantik angewiesen sein.

**Korrektur**

- RGBA-Werte werden an allen konstanten Farbaufrufen explizit über Index 0–3
  übergeben.
- Keine Spread-/Iterator-Annahme mehr an der WebGL-Grenze.

**Validierung**

- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- `git diff --check`: sauber.

### Stand 2026-07-11 — Korrektur nach Abschluss: Rotate-Bounds-Cache

**Fehlerbild**

- Auswahlbox drehte während der Live-Geste korrekt mit, sprang nach dem Commit
  aber auf den alten Bounds-Stand zurück.
- Nach Ab- und Wiederanwählen war die Box korrekt.

**Ursache**

- rotate_selection liest vor der Mutation selection_bbox als Pivot und baut
  dabei den Bounds-Cache für den alten Zustand auf.
- Nach der Rotation wurde dieser Cache nicht erneut invalidiert; Scene lieferte
  deshalb unmittelbar nach dem Commit die alte Auswahlbox.

**Korrektur**

- Scale und Rotate invalidieren abgeleitete Shape-Bounds nach der tatsächlichen
  Geometriemutation.
- Regressionstest prüft nach einer 90-Grad-Gruppenrotation nicht nur das
  Zentrum, sondern auch die erwartete vertauschte Breite/Höhe der Auswahlbox.

**Validierung**

- Rust-Formatierung sauber.
- 198 Core-Tests bestanden.
- Tauri-Backend-Check erfolgreich.
- `git diff --check`: sauber.

### Stand 2026-07-11 — Abschluss ADR 0009

**Umgesetzt**

- Konturen, Füllungen, Gruppenumrisse, Nodes und Bézier-Handles rendern über
  den gemeinsamen WebGL-Pfad.
- Move nutzt Offset-Uniform; Scale/Rotate nutzen Model-Matrix.
- Partielle Fill-Layer-Transformation kombiniert statische und transformierte
  Shape-Ranges in einem Even-Odd-Stencil-Pass.
- Auswahl invalidiert keine Geometrie; Core-Bounds sind gecacht.
- Node-Werkzeug blendet die störende Transform-Bounding-Box aus.
- Render-Takt bleibt auf höchstens einen Animation-Frame begrenzt.

**Mess-/Testabschluss**

- Glyphenlöcher, konkave Formen und reales 1.808-Konturen-DXF visuell korrekt.
- Auswahl des großen DXF von 1.279–2.285 ms auf 56 ms kalt / 16 ms warm
  reduziert; Abwahl 8 ms.
- Move/Pan/Scale/Rotate und partielle Fill-Layer-Auswahl manuell bestätigt.
- Temporäre Performance-Instrumentierung ist zentral deaktiviert; im normalen
  Betrieb entstehen keine Messlogs oder Timerkosten.

**Abschlussvalidierung**

- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- Rust-Formatierung sauber.
- 198 Core-Tests bestanden.
- Tauri-Backend-Check erfolgreich.
- `git diff --check`: sauber.

**Optionale spätere visuelle Parität**

- GPU-Gruppenumrisse sind derzeit durchgezogen statt bildschirmkonstant
  gestrichelt.
- GPU-Node-Marker sind quadratische WebGL-Punkte statt der früheren
  Quadrat-/Kreis-Kombination mit Rand.
- Beides ist funktional bestätigt und kein Performance- oder Korrektheitsblocker.

### Stand 2026-07-11 — Teilschritt 6b: partieller Range-Pfad gemessen

**Bestätigt**

- Kein erneuter Spread-/Iterator-Fehler nach vollständigem Reload.
- Testszene mit 1 ausgewählten von 4 Shapes:
  - Move Geo max 1,00 ms, Durchschnitt 0,17–0,33 ms.
  - Move Gesamt max 2,00 ms, Durchschnitt 0,68–0,92 ms.
  - Rotate Geo max 1,00 ms, Durchschnitt 0,20–0,27 ms.
  - Rotate Gesamt max 1,00 ms, Durchschnitt 0,80–0,91 ms.
- Auswahlantwort in der kleinen Testszene überwiegend 1–2 ms.
- CPU-Füllanteil bleibt 0,00 ms.

**Noch offen**

- Visuell bestätigen, dass bei partieller Auswahl nur die gewählte Füllung
  transformiert wird und alle statischen Formen stehen bleiben.
- Scale wurde in den gelieferten Gestenlogs noch nicht erfasst.
- Großer DXF muss nach Range-Umbau einmal kurz auf Pan-/Vollmove-Regression
  geprüft werden.

### Stand 2026-07-11 — Teilschritt 6c: partieller Range-Pfad abgeschlossen

**Manuell bestätigt**

- Bei partieller Auswahl transformiert ausschließlich die ausgewählte Füllung;
  statische Formen auf demselben Layer bleiben stehen.
- Scale funktioniert korrekt.
- Großes DXF zeigt bei Pan und vollständigem Move keine Regression.

**Ergebnis**

- Move, Scale und Rotate sind für vollständige und partielle Fill-Layer-
  Auswahl GPU-basiert.
- Even-Odd-Parität bleibt layerweit erhalten.
- Kein Auswahl-Reupload und keine Rückkehr zu tausenden Draw-Calls im
  Vollauswahl-/Ruhefall.

**Als Nächstes**

1. Nodes und Bézier-Handles als GPU-Punkt-/Linienbatches umsetzen.
2. CPU-drawNodes entfernen.
3. Node-Edit visuell prüfen, danach Performance-Instrumentierung aufräumen.

### Stand 2026-07-11 — Teilschritt 7: Nodes und Handles auf GPU

**Implementiert**

- Editierknoten werden als gecachter GPU-Punktbatch gezeichnet.
- Bézier-Tangenten liegen als GPU-Linienbatch vor; Handlepunkte liegen im
  gemeinsamen Punktbatch.
- Erster Anker bleibt rot, weitere Anker weiß, Handles blau.
- Batches werden nur bei Scene-/Auswahl-/Werkzeugänderung neu aufgebaut und
  zusammen mit den übrigen GPU-Ressourcen freigegeben.
- Der szenengroße CPU-Zeichner **drawNodes** ist vollständig entfernt.
- CPU-Hit-Test für das Greifen einzelner Knoten bleibt bewusst bestehen; er ist
  Interaktionslogik und kein Zeichenpfad.

**Visuelle Zwischenstufe**

- WebGL-POINTS sind quadratische 9-px-Marker. Die frühere Kombination aus
  weißem Quadrat plus blauem Rand beziehungsweise runden Handlepunkten ist
  farblich angenähert, aber nicht pixelidentisch.

**Statische Validierung**

- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- `git diff --check`: sauber.

**Als Nächstes**

1. Node-Werkzeug an Polyline und Bézier testen.
2. Anker/Handles ziehen und korrekte Live-Aktualisierung prüfen.
3. Danach visuelle Marker-Parität entscheiden und Performance-Messcode
   entfernen.

### Stand 2026-07-11 — Teilschritt 7a: Node-Interaktion bereinigt

**Manuell bestätigt**

- GPU-Nodes und Bézier-Handles funktionieren im Node-Editor.

**Interaktionsregel**

- Im Node-Werkzeug wird die gemeinsame Auswahl-Bounding-Box einschließlich
  Scale- und Rotate-Griffen nicht gezeichnet.
- Anker und Bézier-Handles sind in diesem Modus die alleinigen
  Bearbeitungs-Affordances; die Transformbox würde Greifen und Sicht nur
  behindern.

**Validierung**

- Frontend-Check: 0 Fehler, 0 Warnungen.
- Frontend-Produktionsbuild erfolgreich.
- `git diff --check`: sauber.
