# ADR 0016: Trim-Werkzeug neu aufbauen

## Status

Angenommen und umgesetzt am 2026-07-17. Die praktische Erweiterung für offene
Ketten wurde am selben Tag ergänzt.

## Kontext

Frühere Trim-Varianten waren nicht verlässlich und dienen ausdrücklich nicht
als Referenz. Das Werkzeug wird aus einem kleinen, testbaren Geometrievertrag
neu aufgebaut.

## Entscheidung

- Ein Klick wählt die nächste sichtbare Vektorkontur innerhalb einer
  bildschirmkonstanten Toleranz.
- Alle anderen sichtbaren und entsperrten Vektorkonturen wirken als
  Schneidkanten; Bilder und Hilfskonturen werden ignoriert.
- Entfernt wird der Abschnitt zwischen den nächsten Schnittpunkten vor und
  hinter dem Klick entlang der Zielkontur.
- Bei offenen Zielkonturen gelten neben echten Schnittpunkten auch Anfang und
  Ende der Kette als Begrenzung. Dadurch kann ein einseitiger Überstand bis zum
  nächsten Schnitt entfernt werden.
- Eine offene Kette ohne jeden Schnitt wird beim Klick vollständig entfernt.
- Offene Zielkonturen können null, ein oder zwei Reststücke erzeugen. Bei
  geschlossenen Zielkonturen bleibt der komplementäre offene Restpfad.
- Geschlossene Konturen benötigen weiterhin mindestens zwei Schnittpunkte.
  Bei ineinanderliegenden, berührungslosen Kreisen ist ohne weitere
  Benutzerangabe kein zu entfernender Kreisbogen eindeutig bestimmbar.
- Ein erfolgreicher Klick ist genau ein Undo-Schritt.
- Nach dem Trim wird kein Reststück automatisch ausgewählt, damit wiederholtes
  Bearbeiten nicht durch Auswahlrahmen und Griffe gestört wird.
- Die linke Maustaste kann gehalten werden. Jede beim Überfahren getroffene
  trimbare Kontur wird bearbeitet; der gesamte Mauszug bildet einen einzigen
  Undo-Schritt. Ein bildschirmkonstanter Mindestabstand verhindert, dass ein
  frisch entstandenes Reststück im direkt folgenden Maus-Event erneut trifft.
- Das Ergebnis ist eine normale Polyline. Editierbare Bézier-/Text-Metadaten
  werden bewusst nicht rekonstruiert.
- Hover zeigt den Abschnitt, der beim Klick oder Überfahren entfernt würde,
  als türkisfarbenen Glow mit heller Kernlinie.
- Solange das Trim-Werkzeug aktiv und der Zeiger über dem Canvas ist, ersetzt
  ein nativer transparenter Bitmap-Scheren-Cursor den normalen Systemcursor;
  seine Klingenspitze ist der Trim-Hotspot. Auf Panels und während temporärem
  Verschieben bleibt der jeweilige Standardcursor erhalten.

## Abnahme

Automatisierte Fälle: offene Linie zwischen zwei Schneidkanten, einseitiger
Überstand, freie offene Kette, geschlossene Kontur, geschlossene Kontur mit nur
einem Schnitt, mehrere Schnittpunkte, kein Treffer und Undo. Native zeigt
Vorschau und verwendet dieselbe Core-Berechnung wie der Commit.
