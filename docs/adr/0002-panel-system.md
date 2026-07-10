# ADR 0002: Statisches Bedienlayout, Theming und Arbeitsplatz-Settings

## Status
Akzeptiert — 2026-07-10

## Kontext

Die freie Panel-Positionierung war als Lösung gegen feste Pixel-Layouts gedacht:
Panele konnten pro Reiter verschoben, skaliert, ein-/ausgeblendet und lokal als
Bruchteile des Fensters gespeichert werden.

In der Praxis wurde diese Flexibilität selbst zum Problem. Der Canvas musste aus
beliebigen Panel-Rechtecken ableiten, welche Fläche noch frei ist. Diese
Heuristik war fehleranfällig, insbesondere bei hohen Seitenleisten: FitView
konnte den Arbeitsbereich zu klein einpassen oder an unerwartete Stellen legen.
Außerdem erzeugten `PanelHost`, Editier-Modus, Panel-Toggle, Reset-Logik und
persistierte Layouts mehr Oberfläche und Wartung als Nutzen.

## Entscheidung

LuxiFer verwendet ein statisches, tab-spezifisches Bedienlayout.

- Der Canvas bleibt die Arbeitsgrundlage.
- Bedienflächen sitzen in festen Docks, nicht in frei verschiebbaren Panels.
- `Design` hat feste Zonen für Werkzeuge, Anordnen, Ebenen und Farbpalette.
- Die Formen-Auswahl erscheint nur im Design-Tab, wenn das Polygon-Werkzeug
  aktiv ist.
- `Laser` hat ein festes Laser-Control-Dock.
- `Monitor` hat ein festes rechtes Dock als Platzhalter für Job-Status.
- `Projekt` und `Preview` sind eigene Vollflächen-Ansichten.

Die freien Layoutdaten werden nicht mehr gespeichert. `UiSettings` enthält nur
noch:

- Arbeitsplatzname,
- Theme-Farben samt Intensität,
- zuletzt verwendetes Projekt.

Alte `gui-settings.json`-Dateien mit `layouts` bleiben ladefähig; das Feld wird
ignoriert.

## Invarianten

1. Bedienflächen werden nicht per Nutzer-Drag positioniert oder skaliert.
2. FitView verwendet feste Insets pro Tab, keine Ableitung aus Panel-Rechtecken.
3. Theming kommt weiter aus den GUI-Settings und wird per CSS-Variablen
   angewendet.
4. GUI-Settings bleiben offline lokal persistent; Charon-Sync ist optionaler
   späterer Ausbau.
5. Unbenutzte Layout-Altlasten bleiben nicht im aktiven Codepfad.

## Konsequenzen

- `PanelHost` und der Editiermodus entfallen.
- Es gibt keine gespeicherten Reiter-Layouts, keine Panel-Rechtecke, keinen
  Layout-Reset und kein Panel-Toggle mehr.
- Die Bedien-Komponenten selbst bleiben erhalten (`ToolsPanel`, `LayersPanel`,
  `PalettePanel`, `ShapesPanel`, `ArrangePanel`, `LaserPanel`) und werden direkt
  in statischen Docks gerendert.
- Das Layout ist weniger flexibel, aber stabiler, vorhersehbarer und besser für
  eine CAD-/Laser-Arbeitsoberfläche geeignet.

## Offen / nicht Teil dieser Entscheidung

- Inhaltlicher Ausbau des Monitor-Reiters.
- Optionaler späterer Charon-Sync der verbleibenden GUI-Settings.
