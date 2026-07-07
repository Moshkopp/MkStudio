# LuxiFer — offene Punkte

> **Arbeitsregel bei Unklarheit:** Ist nicht klar, wie eine Funktion arbeiten
> soll, wird die **ThorBurn-Referenz** angeschaut (`nur zur Referenu/` +
> `docs/referenz/`) — **nur analysiert, wie sie gebaut war. Es wird KEIN Code
> übernommen/kopiert.** Neu und sauber im aktuellen Stil implementieren
> (CLAUDE.md, Regel 6).

Legende: `[ ]` offen · `[~]` teilweise · `[x]` erledigt · **(Stub)** =
Platzhalter da, Logik fehlt.

---

## Werkzeuge (Toolbar)

Alle 21 Werkzeuge stehen als Buttons mit SVG-Icon (5 Gruppen nach ThorBurn).
Aktiv: `select`, `rect`, `ellipse`. Der Rest ist **(Stub)** und muss verdrahtet
werden — Fachlogik jeweils in `luxifer-core`, das Frontend zeichnet nur.

### Zeichnen & Formen
- [x] `line` — Linie (offene 2-Punkt-`Geo::Polyline`; Command `add_line`,
      Canvas zeichnet A→B mit Vorschau)
- [x] `polyline` — Polylinie (Klicks setzen Punkte, Gummiband-Vorschau,
      Doppelklick/Enter schließt ab, Escape bricht ab; Command `add_polyline`)
- [ ] `spline` — Spline
- [ ] `bezier` — Bézier-Feder *(war in ThorBurn schon nur Vorschau)*
- [ ] `text` — Text (Glyph→Kontur; größerer Brocken, evtl. eigener Meilenstein)
- [ ] `node` — Knoten/Stützpunkte editieren
- [x] `polygon` mit 9 Formen (tri/quad/penta/hex/octa/star/sun/gear/heart).
      **Kein Flyout** — stattdessen eine datengetriebene Formen-Galerie im
      Werkzeug-Panel (erscheint bei aktivem Polygon-Werkzeug). Katalog kommt aus
      dem Core (`PolyShape::catalog`, Command `shape_catalog`); neue Form = eine
      Enum-Variante in `core/shapes.rs`, kein neuer Button. Zeichnen: Zentrum +
      Aufziehen (Radius), Command `add_polygon` erzeugt geschlossene Polyline.

### Operationen & Hilfsmittel
- [ ] `offset` — Offset/Versatz (in ThorBurn aktiv, hier noch Stub)
- [ ] `measure` — Messen (Messlinie)
- [ ] `trim` — Trimmen *(ThorBurn-Vorschau)*
- [ ] `bridge` — Haltesteg *(ThorBurn-Vorschau)*
- [ ] `boolean` — Boolesche Operationen *(ThorBurn-Vorschau)*
- [ ] `fillet` — Ecken verrunden *(ThorBurn-Vorschau)*
- [ ] `pattern-fill` — Muster füllen *(ThorBurn-Vorschau)*

### Spiegeln
- [x] `mirror_h` — horizontal spiegeln (Sofort-Befehl auf der Auswahl, um die
      Mittelachse der Auswahl-BBox; `Geo::mirror` + `AppState::mirror_selection`)
- [x] `mirror_v` — vertikal spiegeln

### Untersetzer-Schnelleinfügung
- [ ] `coaster_rect` — 4×2 eckige Untersetzer einfügen
- [ ] `coaster_circle` — 4×2 runde Untersetzer einfügen

---

## Projektverwaltung  *(wichtig)*

- [~] **Projekt speichern / laden fertigstellen** — Core-Fundament steht
      (`luxifer-core::project`: `ProjectFile`, `save_to_dir`, `load`,
      `list_projects`), aber **noch keine Tauri-Commands und kein Frontend**.
      Zu tun: Commands `save_project`/`load_project`/`list_projects`/`new_project`
      + AppState laden/ersetzen, dazu die Projektliste im **Projekt-Reiter**
      (anlegen/öffnen/speichern/umbenennen, Liste mit Vorschau).
- [ ] **Import: Bild / SVG / DXF — NUR nach vorheriger Planung!**
      In ThorBurn war der Import **absolut misslungen und fehlerhaft** — bevor
      hier eine Zeile Code entsteht, wird ein sauberes Konzept festgelegt (evtl.
      eigene ADR). ThorBurn-Referenz nur zur Analyse der Fehler, **nichts
      übernehmen**. Reihenfolge: erst Datenmodell/Pfad im Core klären
      (Vektor→Geo, Bild→Raster), dann UI.

## Laser-Preview  *(braucht Neuplanung)*

- [ ] **Preview des Laser-Jobs** — Vorschau von Schnitt-/Füll-/Rasterpfaden,
      Reihenfolge, ggf. Zeit-/Wegschätzung. In ThorBurn **nicht gut gelöst** →
      sauber neu planen, nicht nachbauen. Eigener **Preview-Reiter** (siehe GUI).
      Basis ist der geräteunabhängige `JobPlan` (ADR 0001) — die Preview zeichnet
      den Plan, nicht treiberspezifische Bytes.

## GUI / Panels

- [ ] **Reiter erweitern** auf: `Projekt | Design | Laser | Monitor | Preview`
      (Core `Tab`-Enum + Frontend). Standard-Layout je neuer Reiter ergänzen.
- [ ] **Header über volle Breite:** Reiter **zentriert in der Mitte**; **ganz
      links „LuxiFer" + Logo**, **ganz rechts Zahnrad (Settings)**. Undo/Redo
      neu einordnen (bleiben im Header, aber Layout überdenken).
- [ ] Laser-Panel-Glyphen auf SVG-Icons umstellen (gcode/pause/stop/home/frame/
      contour/send liegen in `Icon.svelte` bereit)
- [ ] Layer-Schalter-Emoji (💨🔥👁) optional auf SVG-Icons (wind/power/eye)
- [ ] `enabled`/`air_assist` auch im Layer-Dialog spiegeln (Konsistenz zur Kachel)
- [ ] Monitor-Reiter mit echtem Job-Status/Fortschritt füllen
- [ ] Jog-Steuerung im Laser-Panel real verdrahten (aktuell Stub)
- [ ] Job-Aktionen Pause/Stopp/Ursprung/Rahmen/Kontur real verdrahten

---

## Settings ausbauen

Das Zahnrad öffnet aktuell nur den Editier-Modus/Theming-Flyout. Settings zu
einem echten Bereich ausbauen:

- [ ] **Laser verwalten** — mehrere Maschinen/Treiber-Profile anlegen (Ruida/
      GRBL/miniGRBL), je mit IP/Ports, Bett-Maßen, Parametern; aktives Profil
      wählbar. Bezug zur Treiber-Abstraktion (ADR 0001) — Core bleibt
      geräteunabhängig, Profile wählen den Treiber.
- [ ] **Canvas-Grid-Size** — Rastergröße des Zeichengitters im Canvas
      einstellbar (Abstand in mm), aktuell fest in `Canvas.svelte` (`drawGrid`).
      Optional Snap-an-Raster fürs Zeichnen/Verschieben. Betrifft den Canvas,
      nicht die Panel-Positionierung.
- [ ] **About** — Info-Dialog mit **automatisch fortlaufender Versionsnummer**
      (aus `Cargo.toml`/`package.json` bzw. Git zur Build-Zeit ziehen, nicht
      manuell pflegen), Lizenz/Autor.
- [ ] **Backup / Restore (Stub)** — Sicherung/Wiederherstellung von Projekten +
      GUI-Settings. Vorerst nur Platzhalter/Stub; späterer Bezug zu Charon
      (Sync/Backup pro Arbeitsplatz).

---

## Core / Treiber

- [~] **Ruida-Control fertigstellen** — Transport (UDP-Ping/Send) an echter HW
      verifiziert, Job-Aufbau (Preamble/Config/Geometrie/Trailer) steht. Offen:
      **realer Brand-Job end-to-end testen**, Live-Steuerung (Start/Pause/Stopp/
      Ursprung/Rahmen/Kontur/Jog) real verdrahten, Statusabfrage (Position/
      Zustand), weitere Startmodi (Anker-Offset) + Fokus-Z. Bleibt im Treiber-
      Crate, Core geräteunabhängig (ADR 0001). Siehe auch GUI: Job-Aktionen/Jog.
- [ ] Raster-/Bildunterstützung im JobPlan (Gravur von Bitmaps)
- [ ] miniGRBL-Treiber (dritter neben Ruida/GRBL) — siehe ADR 0001
- [ ] Projektformat: Bilder mit speichern (folgt mit dem Raster-Teil)

---

## Charon (Server)

- [ ] Bleibt vorerst leer. Später: GUI-Settings pro Arbeitsplatzname
      synchronisieren (JSON-Struktur aus ADR 0002 ist dafür schon vorbereitet).
      Charon steuert **nie** eine Maschine (CLAUDE.md-Invariante 5).
