# ADR 0005: Laser-Preview (Vorschau der Laserpfade)

## Status
Akzeptiert — 2026-07-09

## Kontext

Bisher zeigt LuxiFer nur das **Design**: die Shapes, die der Nutzer zeichnet.
Was der Laser daraus **tatsächlich fährt**, ist unsichtbar — die Kontur-Fahrwege
(Cut), die Flächen-Scanlines (Fill), später die Rasterzeilen eines Bildes und die
**Reihenfolge** samt **Leerfahrten** dazwischen. Vor dem ersten echten Job will
der Nutzer sehen, *was und in welcher Reihenfolge* gebrannt wird, um Fehler
(vergessener Layer, absurde Verfahrwege, falsche Füllung) **vor** dem Laser zu
erkennen.

Die Architektur gibt den sauberen Aufhänger vor: ADR 0001 hat den
geräteunabhängigen **`JobPlan`** eingeführt — Shapes + Layer werden im Core zu
mm-Pfaden pro Layer kompiliert, bevor ein Treiber sie in Ruida-Bytes oder G-Code
übersetzt. **Die Preview ist genau die Visualisierung dieses `JobPlan`** — nicht
eine zweite, eigene Pfad-Berechnung. Damit hat die Preview dieselbe Wahrheit wie
der spätere echte Job: Was die Vorschau zeigt, ist per Konstruktion das, was der
Treiber bekommt.

**Stand des `JobPlan` heute** (`luxifer/core/src/job.rs`): `LayerWork::Cut`
(Kontur-Pfade) und `LayerWork::Fill` (Scanline-Segmente, Even-Odd) existieren und
sind getestet. **`Raster` fehlt noch** (auskommentiert, an den Bild-Job aus ADR
0004 §5 gekoppelt). Ebenfalls **noch nicht modelliert**: die **Reihenfolge** der
Arbeit und die **Verfahrwege (Leerfahrten)** zwischen Pfaden — der `JobPlan`
gruppiert heute nur nach Layer.

Ein Punkt aus ThorBurn (Referenz): Dort war die „Simulation" an die Canvas-Anzeige
und teils an gerätespezifische Details geknüpft. Das vermeiden wir — die Preview
liest **nur** den geräteunabhängigen `JobPlan`, nie einen Treiber.

**Fehlende Layer-Reihenfolge (heute).** Es gibt bislang **keine** vom Nutzer
kontrollierte Layer-Reihenfolge. Die `layers`-Liste entsteht als Nebenprodukt
davon, welche Farbe zuerst benutzt wurde (`find_or_create_layer` /
`layer_for_new_shape` hängen ans Ende an), und der `JobPlan` übernimmt genau diese
Reihenfolge als **Brenn-Reihenfolge**. Damit hängt die Reihenfolge, in der
gefüllt/gschnitten wird, vom Zufall der Farbwahl ab. Für einen echten Job ist das
falsch: Man will bewusst steuern, was zuerst brennt (typisch **erst
füllen/gravieren, dann schneiden** — sonst ist das Werkstück durchtrennt, bevor
graviert wird). Die Preview macht diese Reihenfolge sichtbar — deshalb wird die
**explizite Layer-Reihenfolge zusammen mit der Preview** eingeführt (dieses ADR).

## Entscheidung

### 0. Layer-Reihenfolge = Brenn-Reihenfolge (explizit, vom Nutzer steuerbar)

Die **Position eines Layers in der `layers`-Liste IST die Brenn-Reihenfolge**:
Index 0 brennt zuerst, das letzte Element zuletzt. Der `JobPlan` liest diese
Reihenfolge (tut er heute schon) — neu ist, dass der Nutzer sie **explizit
umsortieren** kann, statt sie dem Zufall der Farbwahl zu überlassen.

- **Kein `order`-Feld.** Die Vektor-Reihenfolge selbst ist die Ordnung; kein
  separates Sortier-Feld, das synchron gehalten werden müsste.
- **Bedienung: Drag & Drop im Layer-Panel.** Die Layer-Kacheln lassen sich per
  Ziehen umsortieren. Die Reihenfolge im Panel = die Reihenfolge in `layers` =
  die Brenn-Reihenfolge.
- **Core-API:** `AppState::move_layer(from: usize, to: usize)` verschiebt einen
  Layer an eine neue Position. **Kritisch:** Weil Shapes ihren Layer per **Index**
  (`shape.layer_id`) referenzieren, muss `move_layer` in **derselben Operation
  alle betroffenen `shape.layer_id` remappen**, sodass jede Shape nach dem
  Verschieben auf denselben Layer wie vorher zeigt. Das ist die eine Stolperstelle
  des index-basierten Modells und wird zentral im Core gelöst (nicht im Frontend).
- **Undo:** `move_layer` ist eine mutierende Aktion → `push_undo` davor (CLAUDE.md
  Regel 4). Ein Umsortieren ist per Undo zurücknehmbar.
- **`active`/Auswahl bleiben erhalten:** Nach dem Verschieben zeigen aktiver
  Layer und Shape-Auswahl weiterhin auf dieselben Layer/Shapes (über das
  Remapping), nicht auf denselben Index.

- **Immer manuell, keine Auto-Sortierung.** Die Reihenfolge wird **ausschließlich**
  vom Nutzer gesetzt. Es gibt bewusst **keinen** automatischen Vorschlag (etwa
  „Fill/Raster vor Cut") — weder beim ersten Job noch beim Import. Die Reihenfolge
  ist die, die im Panel steht, sonst nichts.

Damit ist die Reihenfolge eine **echte Modell-Eigenschaft** (persistiert im
Projekt, da `layers` serialisiert wird), keine reine Anzeige — und die Preview
(§1) zeigt sie als Ausführungsreihenfolge.

### 1. Preview = Visualisierung des `JobPlan` (eine Wahrheit)

Der Core leitet aus dem `JobPlan` eine **Preview-Repräsentation** ab: die zu
fahrenden Linien in **mm**, in **Ausführungs-Reihenfolge**, inklusive der
**Verfahrwege** (Leerfahrten mit Laser aus) dazwischen. Das **Frontend zeichnet
nur** diese Segmente (CLAUDE.md Regel 1 & 2). Es gibt **keine** zweite
Pfad-Berechnung im Frontend.

Neuer Core-Typ (UI-frei, testbar), abgeleitet aus `JobPlan`:

```rust
/// Ein Bewegungssegment der Vorschau in mm, in Ausführungsreihenfolge.
struct PreviewMove {
    from: Pt,
    to: Pt,
    kind: MoveKind,        // Cut | Fill | Raster | Travel
    layer_id: usize,       // welcher Layer (für Einfärbung/Filter); Travel: Ziel-Layer
    seq: u32,              // globaler Reihenfolge-Index (0..n) für Reihenfolge/Scrubber
}

enum MoveKind {
    Cut,      // Kontur fahren (Laser an)
    Fill,     // Scanline fahren (Laser an)
    Raster,   // Rasterzeile fahren (Laser moduliert) — folgt mit Bild-Job
    Travel,   // Leerfahrt (Laser aus) zwischen zwei Arbeitssegmenten
}

/// Die komplette Vorschau eines Jobs.
struct JobPreview {
    moves: Vec<PreviewMove>,
    bbox: Option<(f64, f64, f64, f64)>,   // aus JobPlan übernommen
    total_len_mm: f64,                     // Summe aller Segmentlängen (Arbeit + Travel)
}
```

- **Ableitung im Core:** `JobPreview::from_plan(&JobPlan) -> JobPreview`. Sie
  läuft die `JobLayer` in Plan-Reihenfolge durch, wandelt `Cut`-Pfade und
  `Fill`-Segmente in `PreviewMove`s und **fügt die Verfahrwege ein**: vom
  Endpunkt des vorigen Arbeitssegments zum Startpunkt des nächsten (`Travel`).
  `seq` wird global hochgezählt.
- **Reihenfolge = Plan-Reihenfolge = Layer-Reihenfolge (§0).** Die Preview
  erfindet keine eigene Optimierung; sie zeigt die Reihenfolge, in der der
  `JobPlan` die Arbeit auflistet — und die ist die vom Nutzer gesetzte
  Layer-Reihenfolge (§0). Ändert der Nutzer die Layer-Reihenfolge, ändert sich
  die Preview entsprechend. Wenn später zusätzlich eine Pfad-Sortierung
  *innerhalb* eines Layers (kürzeste Leerfahrten) in den `JobPlan` einzieht, zeigt
  die Preview auch sie **automatisch** — weil sie denselben Plan liest. Das ist
  der Sinn der einen Wahrheit.
- **Travel ist explizit.** Leerfahrten sind ein eigener `MoveKind`, damit die
  Vorschau sie sichtbar (gestrichelt/blass) von der Arbeit trennen kann — genau
  hier erkennt der Nutzer unsinnige Verfahrwege.

### 2. Eigener Preview-Reiter

Die Vorschau bekommt einen **eigenen Reiter** (neben Design/…), keinen Toggle auf
dem Design-Canvas. Begründung: klare Trennung „Entwurf" ↔ „was der Laser tut",
voller Platz für die Wiedergabe-Steuerung (§4) und keine Vermischung der
Design-Interaktion (Auswahl/Resize) mit der reinen Betrachtung.

- Der Reiter zeigt einen **eigenen Canvas**, der `JobPreview` rendert — dieselbe
  mm→Pixel-Transformation/Kamera wie der Design-Canvas (gleiche Bounding-Box,
  gleicher Zoom-Anker), damit Design und Pfad **deckungsgleich** liegen.
- Der Reiter ist ein **reiner Betrachter**: keine Bearbeitung, kein Verschieben
  von Shapes. Eingaben beschränken sich auf Ansicht (Zoom/Pan) und Wiedergabe.
- **Datenfluss:** Wechsel auf den Reiter (oder Änderung von Shapes/Layern) →
  Tauri-Command holt `JobPreview` aus dem Core → Frontend zeichnet. Kein
  Vorschau-Zustand als eigene Wahrheit im Frontend; die Segmente sind Cache
  der aktuellen `AppState`.

### 3. Umfang: Cut + Fill + Raster — Architektur komplett, Umsetzung gestaffelt

Die Preview-**Architektur** deckt alle drei Arbeitsarten ab (`MoveKind::Cut`,
`Fill`, `Raster`). **Implementiert** wird nach Verfügbarkeit im `JobPlan`:

- **Cut** — `LayerWork::Cut` existiert → **jetzt**.
- **Fill** — `LayerWork::Fill` (Scanline) existiert → **jetzt**.
- **Raster** — `LayerWork::Raster` existiert **noch nicht** (an den Bild-Job aus
  ADR 0004 §5 gekoppelt). `MoveKind::Raster` wird im Typ **angelegt**; die
  Ableitung füllt es, **sobald** der JobPlan Rasterzeilen liefert. Bis dahin
  erscheinen Bild-Layer in der Preview über ihre Kontur (wie im JobPlan heute:
  Bild-Box als geschlossener Pfad).

So ist die Preview **jetzt** für den realen Funktionsumfang (Cut+Fill) baubar,
ohne Raster-Geometrie zu erfinden, die es noch nicht gibt — und wächst ohne
Umbau mit, wenn der Bild-Job kommt.

### 4. Wiedergabe: statisch + Reihenfolge jetzt, Play/Scrubber vorgesehen

**Diese Stufe:** statische Anzeige aller Segmente mit **sichtbarer Reihenfolge**.
Umsetzung im Frontend aus `seq`:

- **Reihenfolge über Farbverlauf**: Segmente werden entlang `seq` von „früh" nach
  „spät" eingefärbt (Verlauf), sodass Start→Ende auf einen Blick lesbar ist.
- **Travel-Segmente** blass/gestrichelt, Arbeit (Cut/Fill/Raster) kräftig.
- Optional pro Layer ein-/ausblendbar (Filter über `layer_id`).

**Play/Pause + Scrubber** (Laserkopf fährt die Pfade animiert ab) sind **im Typ
schon vorbereitet** — `seq` ist der Zeitindex, `PreviewMove` trägt Länge/Kind für
ein späteres Zeitmodell (Weg ÷ `speed_mm_s` pro Layer). Die animierte Wiedergabe
ist aber **nicht Teil dieser Stufe** (siehe „Nicht Teil dieser Entscheidung").
Der Reiter reserviert dafür bereits den Platz einer Steuerleiste.

## Invarianten

1. **Preview = `JobPlan`-Visualisierung, keine zweite Wahrheit.** Die Preview
   wird ausschließlich aus dem `JobPlan` abgeleitet; sie berechnet keine eigenen
   Pfade. Was sie zeigt, ist per Konstruktion das, was der Treiber kompiliert.
2. **Ableitung im Core, Frontend zeichnet nur** (CLAUDE.md Regel 1 & 2).
   `JobPreview::from_plan` ist UI-frei und testbar; das Frontend rendert nur
   `PreviewMove`s und hält keinen Wahrheits-Zustand.
3. **Kein Gerätecode in der Preview.** Sie liest den geräteunabhängigen Plan, nie
   einen `MachineDriver` (ADR 0001 Invariante 1).
4. **Reihenfolge ist Plan-Reihenfolge.** Die Preview ordnet nicht selbst um; eine
   spätere Fahrweg-Optimierung gehört in den `JobPlan` und schlägt dann
   automatisch auf die Preview durch.
5. **Layer-Reihenfolge = Vektor-Reihenfolge = Brenn-Reihenfolge.** Es gibt kein
   separates `order`-Feld. `move_layer` **remappt alle `shape.layer_id`** in einer
   Operation (unter `push_undo`), sodass Shapes nach dem Umsortieren auf denselben
   Layer zeigen wie vorher. Das Remapping liegt im Core, nicht im Frontend.
6. **Verfahrwege sind explizit** (`MoveKind::Travel`), sichtbar von Arbeit
   getrennt — der Kern-Nutzen der Vorschau.
7. **Aus ThorBurn wird kein Code kopiert** (CLAUDE.md Regel 6).

## Konsequenzen

- `AppState` bekommt `move_layer(from, to)` mit `shape.layer_id`-Remapping und
  `push_undo`; Tests für: Reihenfolge geändert, Shapes zeigen weiter auf ihre
  Layer, aktiver Layer/Auswahl erhalten, Undo stellt die alte Reihenfolge her.
- Das Layer-Panel bekommt **Drag & Drop** zum Umsortieren; ruft `move_layer` als
  Tauri-Command. Frontend zeichnet nur, die Ordnung lebt im Core.
- Die Layer-Reihenfolge wird über die bestehende `layers`-Serialisierung
  automatisch **im Projekt persistiert** (ADR 0003) — kein neues Feld.
- Neues Core-Modul (z. B. `luxifer/core/src/preview.rs`) mit `JobPreview`,
  `PreviewMove`, `MoveKind` und `JobPreview::from_plan`; Tests für Reihenfolge,
  eingefügte Travel-Segmente und Cut/Fill-Abdeckung.
- Neuer Tauri-Command, der aus der aktuellen `AppState` den `JobPlan` und daraus
  `JobPreview` baut und ans Frontend gibt.
- Neuer Preview-Reiter im Frontend (eigener Canvas, gemeinsame Kamera mit dem
  Design-Canvas), der die Segmente mit Reihenfolge-Verlauf zeichnet.
- Der `JobPlan` bleibt unverändert; die Preview ist eine **abgeleitete Sicht**.
  Wenn Reihenfolge/Optimierung oder `LayerWork::Raster` in den Plan kommen, wächst
  die Preview ohne Schnittstellenbruch mit (`MoveKind` ist bereits vollständig).

## Nicht Teil dieser Entscheidung

- **Animierte Wiedergabe** (Play/Pause/Scrubber, fahrender Laserkopf) — Typ ist
  vorbereitet (`seq`, Segmentlänge/Kind), umgesetzt wird sie später mit einem
  Zeitmodell (Weg ÷ `speed_mm_s`, inkl. Beschleunigung optional).
- **Raster-Segmente** — `MoveKind::Raster` ist angelegt, aber erst befüllbar, wenn
  der Bild-Job (`LayerWork::Raster`, ADR 0004 §5) existiert.
- **Fahrweg-Optimierung** (Pfad-Sortierung für kürzeste Leerfahrten) — gehört in
  den `JobPlan`, nicht in die Preview; die Preview zeigt dann automatisch die
  optimierte Reihenfolge.
- **Zeit-/Dauer-Schätzung** des Jobs (Gesamtzeit aus Weg und Speed) — naheliegende
  Erweiterung auf `total_len_mm` + Speed, aber nicht jetzt.
- **Overlay-Modus auf dem Design-Canvas** — bewusst zugunsten des eigenen Reiters
  verworfen; könnte später als zusätzliche Ansicht ergänzt werden.
- **Automatische Layer-Sortierung** (z. B. „Fill/Raster vor Cut") — bewusst
  verworfen; die Reihenfolge ist immer manuell (§0).
- **Feintuning** von Preview und Reihenfolge — kommt in einem **eigenen zweiten
  ADR (0006)** nach diesem Schritt (z. B. animierte Wiedergabe, Zeit-Schätzung,
  Fahrweg-Optimierung, Darstellungs-Details). Dieses ADR setzt die Grundlage.
