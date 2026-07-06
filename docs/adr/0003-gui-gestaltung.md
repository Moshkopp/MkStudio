# ADR 0003: GUI-Gestaltung und Layout

## Status
Akzeptiert — 2026-07-06

## Kontext

Der aktuelle LuxiFer-Prototyp ist funktional, aber gestalterisch roh: feste
Icon-Werkzeugleiste links, ein einziges rechtes Panel, das Layer, Parameter und
Auswahl als lange Liste stapelt, kein Lineal, keine Farbpalette, viel
ungenutzter Raum und keine klare visuelle Hierarchie.

Als Referenz dient die Vorgänger-App **ThorBurn** (siehe
[[thorburn-reference]] / `nur zur Referenu/`). Sie war funktionstüchtig, aber
schlecht gebaut — ihr **Layout** ist jedoch ausgereift und ein gutes Leitbild:
Lineale am Canvas, schwebende gruppierte Werkzeugpaletten, ein Design/Laser-
Moduswechsel, ein kompaktes Tabellen-Layer-Panel mit Maschinenparametern, eine
Farbpalette zur Layer-Zuordnung und gefüllte Formen im Fill-Modus.

Wir legen fest, an welchen Gestaltungsprinzipien sich LuxiFers GUI orientiert.
Dies betrifft ausschließlich die Präsentationsschicht (`LuxiFer.App`); die
Architekturregeln aus [ADR 0001](0001-gui-first-offline-first.md) und
[CLAUDE.md](../../CLAUDE.md) bleiben unberührt (Fachlogik im Core, dünne UI).

## Entscheidung

**ThorBurns Layout ist das Leitbild.** Konzepte werden übernommen, aber im
aktuellen Stil sauber neu implementiert — niemals Code kopiert.

### 1. Zwei Arbeitsmodi: Design und Laser

Die Oberfläche trennt **Design** (Zeichnen, Formen, Anordnen) von **Laser**
(Maschinenparameter je Layer, Job-Vorbereitung). Umschaltung oben rechts.
Das entlastet das rechte Panel: im Design-Modus stehen Objekt-/Auswahl-
Eigenschaften im Vordergrund, im Laser-Modus die Layer-Maschinenparameter
(Speed, Power, Passes, Air Assist, Modus).

### 2. Canvas mit Linealen und mm-first

- Lineale am oberen und linken Canvas-Rand mit mm-Skala.
- Alle Maße, Positionen und Eingaben in Millimetern (bereits im Core so
  modelliert). Der Canvas ist ein strukturiertes Dokument, kein Bild.
- Cursorposition in mm bleibt in der Statuszeile.

### 3. Werkzeuge und Paletten

- Zeichenwerkzeuge als kompakte Icon-Palette (Select, Rechteck, Ellipse, Linie,
  Polyline, Polygon, später Text/Bild).
- Häufige Aktionen als gruppierte Paletten statt eines überladenen Balkens:
  - **Transform-Palette**: X / Y / Breite / Höhe / Skalierung % / Rotation,
    mit Seitenverhältnis-Sperre.
  - **Anordnen-Palette**: Ausrichten und Verteilen der Auswahl.

### 4. Layer-Panel als Tabelle

Layer werden als kompakte Tabelle dargestellt: Farbfeld, Name, Modus,
Geschwindigkeit/Leistung, plus Umschalter für Sichtbarkeit und Sperre.
Eine **Farbpalette** erlaubt die schnelle Zuordnung einer Layer-Farbe.

### 5. Fill-Vorschau

Formen auf Fill-/Raster-Layern werden gefüllt dargestellt (halbtransparent in
Layerfarbe), Cut-Layer nur als Kontur. So ist der Bearbeitungsmodus visuell
sofort erkennbar.

### 6. Theme und visuelle Sprache

- Dunkles Theme als Standard (Werkstattumgebung, kontrastarm für lange Sitzungen).
- Klare Hierarchie durch Abschnittsüberschriften, Trenner und einheitliche
  Abstände; kein ungenutzter Leerraum durch fehlende Gruppierung.
- Konsistente Farbpalette; Layerfarben aus der bestehenden `SwatchColors`-Liste.

## Konsequenzen

- Die Gestaltung wird schrittweise umgesetzt; dieses ADR ist der Zielzustand,
  nicht eine einzelne Umbaumaßnahme. Jeder Schritt hält die Architekturregeln
  ein (Fachlogik im Core, `*.axaml.cs` dünn).
- Der Design/Laser-Modus wird als Zustand im ViewModel geführt und blendet die
  jeweils passenden Panels ein.
- ThorBurns *Verhalten* ist Referenz für Feature-Umfang; sein Code bleibt tabu
  (Regel 11 der CLAUDE.md).
- Abweichungen von diesen Prinzipien oder wesentliche Layout-Neuentscheidungen
  werden als weiteres ADR festgehalten.

## Nicht Teil dieser Entscheidung

Konkrete Pixelmaße, finale Icons und die Reihenfolge der Umsetzung. Features
wie Text, Bildimport, Nesting oder boolesche Operationen (in ThorBurn
vorhanden) sind hier nur als Ausblick genannt, nicht beschlossen.
