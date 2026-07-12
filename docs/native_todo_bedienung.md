# Native-Bedienung: Mängelliste & TODO

Stand: 2026-07-12. Vom Nutzer am laufenden Fenster gesammelt. Diese Liste wird
analysiert, priorisiert und abgearbeitet. Klassifizierung:

- **REG** = Regression: ging in der Tauri-App, durch die native Migration
  verloren/kaputt.
- **FEHLT** = Feature war/ist noch nicht nativ umgesetzt (bekannte Lücke).
- **UX** = vorhanden, aber unbrauchbar/unklar/hässlich.
- **BUG** = klar falsches Verhalten.

Priorität: P1 = blockiert normales Arbeiten, P2 = wichtig, P3 = Politur.

---

## A. Canvas / Zeichnen / Auswahl

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| A1 | BUG | P1 | Auswahl-Werkzeug zeichnet keinen Marquee-Rahmen — man zieht blind. |
| A2 | REG | P1 | Bézier-Tool arbeitet nicht wie in Tauri (Inkscape-like): Klick + gehaltene Maustaste erzeugt Anker und zieht die Tangente/Kurve mit der Maus. |
| A3 | UX | P2 | Spline/Polyline/Bézier: Klick auf ersten Node ODER Enter schließt die Form; Startnode leuchtet farbig, wenn die Maus in die Nähe kommt (kein Zielen nötig). |
| A4 | REG | P1 | Strg+Z / Strg+Shift+Z gehen nicht (Undo/Redo per Tastatur). |
| A5 | UX | P3 | Undo/Redo sollen als Icons in den Header. |

## B. Geometrie-Operationen

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| B1 | BUG | P1 | Offset macht aus harten Kanten ein Offset mit Fills (falsches Ergebnis). |
| B2 | FEHLT | P2 | Musterfüllung ist nur Stub. |
| B3 | FEHLT | P2 | Haltesteg ist nur Stub. |

## C. Bilder

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| C1 | BUG | P1 | Bilder werden rosa/pink dargestellt (falsches Textur-Format/Sampling). |
| C2 | UX | P2 | Bild-Doppelklick-Dialog hat keine Live-Vorschau der Einstellungen. |
| C3 | FEHLT | P2 | Bildfunktionen fehlen: Vektorisieren (Trace), Zuschneiden. |

## D. Fills / Vorschau

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| D1 | BUG | P1 | Fills werden schon im Design-Tab als Raster dargestellt (sollten Fläche/Kontur sein, Raster nur im Laser/Preview). |
| D2 | FEHLT | P1 | Laser-Preview-Tab fehlt komplett. |

## E. Panels / Layout / Views

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| E1 | UX | P1 | Rechtes Panel (Laser/Design) zu schmal — Inhalt wird abgeschnitten. |
| E2 | UX | P2 | Layerliste schwer deutbar — muss vernünftig gestaltet werden. |
| E3 | BUG | P2 | Im Laser-Tab kann man weiterhin zeichnen, hat aber keine Layerliste mehr. |
| E4 | UX | P1 | Projektmanager unbrauchbar: nur eine überbreite Liste, keine Details, keine Thumbnails. |

## F. Header / Werkzeug-Zugänge

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| F1 | UX | P2 | Vektor- und Bildladen sollen über einen gemeinsamen Import-Button laufen. |
| F2 | UX | P3 | „Aztec laden" und „Text einfügen" gehören in den Header. |

## G. Text

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| G1 | FEHLT | P2 | Text-Tool: keine Vorschau. |
| G2 | FEHLT | P2 | Text-Tool: kein Upload eigener Fonts. |

## H. Canvas-Grid / Lineale

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| H1 | UX | P2 | Canvasgrid soll über den gesamten Body gehen; das Arbeitsfeld wird nur umrandet statt durchgehend gerastert. |
| H2 | FEHLT | P2 | Canvas fehlen Lineale (Ruler oben/links). |

---

## Analyse-Notizen (wird ergänzt)

- A1: `Drag::Marquee` wird in `gestures.rs` gesetzt, aber `canvas/overlay.rs`
  zeichnet dafür kein Rechteck. Reiner Overlay-Fix.
- A4: Shortcut ist verdrahtet (`Key::Z + ctrl → Undo`), Redo nur auf `Key::Y`,
  NICHT auf `Strg+Shift+Z`. Zu prüfen: greift das Fokus-/Modal-Gate zu früh?
