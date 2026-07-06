# ThorBurn-Canvas — Funktionsanalyse (Referenz)

Analyse der Canvas-Logik der Vorgänger-App **ThorBurn** (Qt/QML), als Referenz
für LuxiFer. **Kein Code wird kopiert** (CLAUDE.md Regel 11) — dieses Dokument
beschreibt, *wie* die Funktionen arbeiten, damit wir sie im aktuellen Stil sauber
neu bauen können.

Die Canvas-Logik ist in vier JavaScript-Module aufgeteilt, die der QML-Wrapper
`BedCanvas.qml` orchestriert. Prinzip: Die QML-Datei hält nur den Zustand und
die Event-Weiterleitung; die eigentliche Logik liegt in den Modulen, die ihren
Kontext (`document`, `projects`, `CanvasGeo`, …) **als Parameter** bekommen
(kein impliziter Global-Zugriff).

## Datenmodell (zentral für den Farb-Bug!)

In ThorBurn ist ein Objekt ein einfaches JS-Objekt mit **eigener Farbe**:

```js
{ id, type: "Rect"|"Ellipse"|"Polyline"|"Line"|"Image", color: "#rrggbb", … }
```

- **Jedes Objekt trägt seine eigene `color`.** Beim Erzeugen wird die
  Layer-Farbe nur als *Startwert* kopiert (`layerColor(document)`), danach ist
  die Farbe objekt-eigen.
- Die Farbpalette (BottomBar) setzt `objs[i].color` **nur für die selektierten
  Objekte** → einzelne Objekte können umgefärbt werden.
- Rechteck und Linie werden intern als **geschlossene/offene Polylinie**
  gespeichert (web-GUI-kompatibel), nicht als eigener Rect/Line-Typ.

> **Konsequenz für LuxiFer:** LuxiFer bindet die Farbe an den **Layer**, nicht
> ans Objekt. Deshalb färbt die Palette dort „alle" (den ganzen Layer). Wer
> einzelne Objekte färben will, braucht — wie ThorBurn — eine **objekteigene
> Farbe** (Override) zusätzlich zur Layerfarbe. Das ist eine Architektur-
> entscheidung und gehört in ein ADR.

---

## Modul 1: `CanvasGeometry.js` — Geometrie & Hit-Testing

Reine, zustandslose Geometrie (als `.pragma library` isoliert).

| Funktion | Was sie tut |
|---|---|
| `getBoundingBox(objs)` | Umschließende Box **mehrerer** Objekte (min/max über alle `getObjectBox`). Liefert `{x,y,w,h}` oder `null`. |
| `getObjectBox(o)` | Box **eines** Objekts. Rect/Image: `x,y,w,h`. Ellipse: aus `cx,cy,rx,ry`. Polyline: min/max über alle Punkte (überspringt `null`-Lücken). |
| `hitTestObject(o, wx, wy, tol)` | Trifft ein Weltpunkt das Objekt? Rect/Image: Punkt in Box. Ellipse: normierte Ellipsengleichung ≤ 1. Polyline: Abstand zu jedem Segment ≤ `tol` (bei `closed` auch Schlusssegment). |
| `pointToSegmentDistance(px,py,x1,y1,x2,y2)` | Kürzester Punkt-Segment-Abstand (projiziert `t` auf `[0,1]`, dann `hypot`). Basis des Polyline-Hit-Tests. |
| `hitNode(objs, wx, wy, tol)` | Für das **Knoten-Werkzeug**: liegt der Punkt nahe einem Stützpunkt einer Polyline? Liefert `{objId, index}` oder `null`. |
| `makePolygonPoints(shape, cx, cy, r, rot)` | **Live-Preview** für Polygon-Formen (tri/quad/penta/hex/octa/star/sun/gear/heart). Erzeugt Punkte auf einem Ring bzw. Doppelring (Stern). Die *finalen* Punkte holt ThorBurn vom Backend — eine Quelle der Wahrheit. |
| `catmullRom(pts, segsPerSpan)` | **Live-Preview** der Spline-Glättung (Catmull-Rom-Interpolation, 16 Segmente/Spanne). Finale Kurve wieder vom Backend. |
| `heartPoints(cx,cy,r,rot)` | Parametrisches Herz (100 Punkte), auf `r` normiert und um `rot` gedreht. |

**Kernidee:** Hit-Test ist typ-spezifisch; die Toleranz kommt in *mm* herein
(`6/viewScale` → 6 Pixel unabhängig vom Zoom).

---

## Modul 2: `CanvasInput.js` — Maus-Interaktion & Werkzeuge

Alle Handler bekommen `(c, ev, x)`: `c` = Canvas-Zustand/-Funktionen,
`ev` = Maus-Event, `x` = Kontext (`document, projects, CanvasGeo, …`).

| Funktion | Was sie tut |
|---|---|
| `worldXY(c, ev)` | Screen-Pixel → Welt-mm: `ev.x / viewScale + viewOx`. Umkehrung von `w2s`. |
| `nextId(objs)` | Nächste freie Objekt-ID (`max(id)+1`). |
| `layerColor(document)` | Farbe des ersten Layers als **Startfarbe** neuer Objekte. |
| `onPressed(c, ev, x)` | Zentrale Weiche nach `activeTool`. Mittel-Maustaste oder Space+Links = **Pan** (View verschieben). Sonst je Werkzeug: `select`, `node`, `rect`, `ellipse`, `line`, `polygon`, `polyline`/`spline`, `text`, `measure`. Zeichenwerkzeuge legen ein `newShape` mit Startgröße `0.1` an. |
| `pressNode(...)` | Knoten-Werkzeug: erst Stützpunkt anfassen (→ `nodeDrag`), sonst Objekt selektieren/deselektieren. |
| `pressSelect(...)` | Select-Werkzeug: von **oben nach unten** (`i--`) das erste getroffene Objekt suchen. Shift/Strg = additiv. Merkt sich die **Ausgangspositionen aller selektierten Objekte** (`initialPositions`) für einen verlustfreien Drag. |
| `pressPolyPoint(...)` | Polyline/Spline: Stützpunkt setzen. Das **letzte** Array-Element ist immer die Gummiband-Vorschau. Klick nahe Start (< 5px) schließt/finalisiert. |
| `onPositionChanged(c, ev, x)` | Weiche nach `activeTool` beim Ziehen: Pan, Knoten schieben, Auswahl verschieben (`moveSelection`), Rect/Ellipse/Line/Polygon live aufziehen, Polyline-Gummiband, Mess-Cursor. Jeder Zweig ruft `requestPaint()`. |
| `moveSelection(c, mx, my, document)` | Verschiebt **alle** selektierten Objekte um das Delta zur Drag-Startposition — auf Basis der gemerkten `initialPositions` (kein Fehlerakkumulieren). Typ-spezifisch (Rect: x/y, Ellipse: cx/cy, Polyline: alle Punkte). |
| `onReleased(c, ev, x)` | Schließt die aktive Aktion ab. Zeichnen: nur committen, wenn groß genug (Rect `w,h>1`, Ellipse `rx,ry>0.5`, Line-Länge `>1`, Polygon `r>1`). Rect/Line werden als Polyline gespeichert; Ellipse/Polygon gehen über `projects.addXShape` (Backend). `save()`. |
| `finishDraw(c)` | Räumt `newShape` auf und schaltet zurück auf `select`. |
| `onDoubleClicked(c, ev, x)` | Select: Doppelklick auf ein **Bild** öffnet dessen Eigenschaften. Polyline/Spline: Doppelklick **finalisiert** den offenen Pfad (mit Sonderbehandlung des doppelten Endpunkts). |
| `onWheel(c, wheel)` | Zoom um den Mauszeiger: `viewScale *= 1.15/0.85` (geклemmt `0.02…80`), dann `viewOx/Oy` so anpassen, dass der Punkt unter der Maus fix bleibt. |

**Kernидee:** Ein einziger `activeTool`-String steuert alle Handler. Zeichnen
ist immer „Aufziehen mit Live-Preview → beim Loslassen committen, wenn groß
genug → zurück auf select".

---

## Modul 3: `CanvasPaint.js` — Zeichnen (Rendering)

Bekommt Kontext als Parameter (bewusst **kein** `.import`, damit die QML-Context-
Properties sichtbar bleiben). Alles über einen 2D-Canvas-`ctx`.

| Funktion | Was sie tut |
|---|---|
| `paint(c, Theme, document, CanvasGeo)` | **Einstieg.** Zeichnet die ganze Szene in fester Reihenfolge: Hintergrund → Gitter → Objekte → Zeichen-Vorschau → Polyline-Vorschau → Selektion → Knoten (nur Node-Tool) → Messlinie → Lineale. Definiert `w2s(x,y)` (Welt→Screen). |
| `drawGrid(ctx, w2s, bedW, bedH)` | Feines 10mm-Gitter (sehr transparent), grobes 100mm-Gitter, Bett-Grenze als Rechteck. |
| `drawObjects(ctx, c, w2s, Theme, document)` | Alle Objekte. **`strokeStyle = o.color`** (objekteigene Farbe!). Rect/Ellipse/Polyline/Image typ-spezifisch. Ellipse rechnet cx/cy/rx/ry in Qts „obere-Ecke+Größe"-Ellipse um. |
| `drawImageObject(...)` | Bild mit progressivem Nachladen der Zoomstufe (`thumbUrl`/`baseThumbUrl`), Umriss (bei Auswahl akzentuiert), Dateiname-Label. |
| `drawNewShapePreview(...)` | Gestrichelte Live-Vorschau des gerade aufgezogenen Rect/Ellipse/Line/Polygon. |
| `drawPolylinePreview(...)` | Gestrichelte Vorschau des aktiven Pfades; bei Spline via `catmullRom` geglättet. |
| `drawSelection(...)` | Gemeinsame Bounding-Box **aller** selektierten Objekte + **8 Resize-Handles** (Ecken/Kantenmitten) als weiße Quadrate. **Keine Rotation** (ThorBurn dreht nicht per Maus). |
| `drawNodes(...)` | Stützpunkte der selektierten Polylinien als Quadrate (erster Punkt akzentuiert). |
| `drawMeasure(...)` | Messlinie mit zwei Punkten + Label (Länge, Δx, Δy). |
| `drawRulers(...)` | Lineale oben/links, Tick-Intervall zoomabhängig (2/5/10/20/50/100mm), Major-Ticks beschriftet. |

**Kernidee:** Ein `paint()` zeichnet immer die *ganze* Szene neu (Immediate-Mode
Canvas); Reihenfolge = Ebenen. Objektfarbe kommt aus `o.color`.

---

## Modul 4: `CanvasTools.js` — Nicht-interaktive Operationen

| Funktion | Was sie tut |
|---|---|
| `mirror(horizontal, c, document, projects, CanvasGeo)` | Spiegelt die selektierten Objekte an der Mittelachse ihrer gemeinsamen Bounding-Box. Da absolute Punkte gespeichert werden: `x → 2*cx - x` (Polyline punktweise; Rect an oberer Ecke; Ellipse am Mittelpunkt). |
| `insertCoasterGrid(shape, c, document, projects)` | Fügt ein 4×2-Raster aus 100mm-Formen (20mm Abstand), zentriert aufs Bett, ein. `rect` = 4-Punkt-Polyline, `circle` = 64-Segment-Polyline. Nimmt die erste Layerfarbe. |

---

## Übertragung auf LuxiFer — was das für uns heißt

1. **Objektfarbe vs. Layerfarbe** — der eigentliche Unterschied. ThorBurn:
   Farbe pro Objekt. LuxiFer: Farbe pro Layer. Für „einzelnes Objekt umfärben"
   brauchen wir eine **optionale Objekt-Farb-Override**. → eigenes ADR.
2. **Hit-Test/Bounds** liegen bei uns schon sauber im Core (testbar) — deckungs-
   gleich mit ThorBurns `hitTestObject`/`getObjectBox`.
3. **Verschieben mehrerer Objekte** über gemerkte Startpositionen — machen wir
   bereits (Gruppen-Move via Deltas).
4. **Rotation per Maus** hatte ThorBurn **nicht** — unser Dreh-Griff ist eine
   Verbesserung.
5. **Werkzeuge, die uns noch fehlen** (aus ThorBurn): Knoten-Editor, Spline/Pen,
   Polygon-Formen (Stern/Zahnrad/Herz), Messwerkzeug, Spiegeln, Bildimport,
   Text, Untersetzer-Raster.
