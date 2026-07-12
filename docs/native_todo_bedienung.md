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
| A1 | ERLEDIGT | P1 | Auswahl-Werkzeug zeichnet einen bildschirmkonstant gestrichelten Marquee-Rahmen. |
| A2 | ERLEDIGT | P1 | Bézier-Feder: Drücken setzt Anker, Ziehen erzeugt symmetrische Tangenten; Enter schließt den Entwurf ab. |
| A3 | ERLEDIGT | P2 | Spline/Polyline/Bézier rasten nahe dem Startknoten ein; Klick oder Enter schließt den Pfad, der Startknoten signalisiert die Fangzone farbig. |
| A4 | ERLEDIGT | P1 | Strg+Z = Undo, Strg+Shift+Z und Strg+Y = Redo. |
| A5 | UX | P3 | Undo/Redo sollen als Icons in den Header. |

## B. Geometrie-Operationen

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| B1 | ERLEDIGT | P1 | Geschlossene konvexe Linienkonturen behalten beim Offset harte Miter-Ecken statt verrundeter Übergänge. |
| B2 | FEHLT | P2 | Musterfüllung ist nur Stub. |
| B3 | FEHLT | P2 | Haltesteg ist nur Stub. |

## C. Bilder

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| C1 | ERLEDIGT | P1 | Bildtexturen werden nicht mehr von Scanlines der pinken Bild-Layer-Kennfarbe überzeichnet. |
| C2 | UX | P2 | Bild-Doppelklick-Dialog hat keine Live-Vorschau der Einstellungen. |
| C3 | FEHLT | P2 | Bildfunktionen fehlen: Vektorisieren (Trace), Zuschneiden. |

## D. Fills / Vorschau

| ID | Klasse | Prio | Beschreibung |
|----|--------|------|--------------|
| D1 | ENTSCHIEDEN | — | Scanlines bleiben bewusst im Design-Tab: direkte Kontrolle des Fill-Ergebnisses; der native Vertex-Cache zeigt aktuell keinen spürbaren Performance-Einbruch. |
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

- A1 (erledigt): `Drag::Marquee` wird als gestricheltes, jeden Frame neu
  aufgebautes Overlay gezeichnet; der Geometrie-Cache bleibt auswahlfrei.
- A4 (erledigt): `Strg+Shift+Z` wird vor `Strg+Z` ausgewertet und löst Redo
  aus. `Strg+Y` bleibt als Alias erhalten; Fokus-/Modal-Gate bleibt wirksam.
- A2 (erledigt): Der Canvas hält während des Zeichnens echte `BezierNode`-Drafts.
  Beim Drag entstehen `h_in`/`h_out`, das Overlay zeigt Kurve, Tangenten und
  Anker live; die Application übernimmt den fertigen Pfad als einen Undo-Schritt.
- A3 (erledigt): Eine bildschirmkonstante 10-px-Fangzone schließt Pfade ab drei
  Knoten. Overlay-Gummiband und Startmarker zeigen das Einrasten; Application
  erzeugt für Klick und Enter echte geschlossene Polyline-/Spline-/Bézier-Pfade.
- B1 (erledigt): `cavalier_contours` erzeugte standardmäßig runde Außen-Joins.
  Geschlossene konvexe Linienkonturen nutzen nun im Core Schnittpunkte
  benachbarter Parallelkanten (Miter); kollabierte Innenoffsets bleiben leer.
  Konkave und offene Konturen behalten die robuste Selbstschnittbehandlung.
- C1 (erledigt): RGBA-Textur und Shader waren korrekt. Der nachfolgende native
  Vektor-Fill-Pass behandelte jedoch `LayerMode::Image` als Füllkontur und malte
  die rechteckige Bildfläche in der Layer-Kennfarbe über. Zusätzlich lag die
  Textur vor dem opaken Bett. Die Reihenfolge ist nun Bett/Gitter → Bildtexturen
  → Vektorgeometrie → Overlay; Image-Layer erzeugen keine Fill-Scanlines mehr.
- D1 (bewusst beibehalten): Anders als zunächst geplant bleiben Fill-Scanlines
  im Design-Tab sichtbar. Sie liefern sofortige visuelle Kontrolle über das
  tatsächliche Fill-Ergebnis; dank gecachtem Vertexpuffer ist derzeit kein
  wahrnehmbarer Performanceverlust vorhanden. Nur bei belegbarer Regression
  erneut aufgreifen.
