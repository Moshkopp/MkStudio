# ADR 0005: Farbe gehört zum Objekt

## Status
Akzeptiert — 2026-07-06

## Kontext

Bisher bestimmte in LuxiFer der **Layer** die Farbe (`Layer.ColorHex`); alle
Objekte eines Layers wurden in dieser Farbe gezeichnet. Das machte es unmöglich,
**einzelne Objekte** individuell einzufärben — die Farbpalette färbte immer den
ganzen Layer.

Die Referenz **ThorBurn** (siehe
[thorburn-canvas-analyse.md](../thorburn-canvas-analyse.md)) löst das anders:
**jedes Objekt trägt seine eigene Farbe.** Beim Erzeugen wird die Layerfarbe nur
als *Startwert* kopiert; danach ist die Farbe objekt-eigen und pro Auswahl
änderbar.

Wir übernehmen dieses Modell.

## Entscheidung

**Die Farbe ist eine Eigenschaft des Objekts, nicht des Layers.**

1. `CanvasObject` erhält `ColorHex` (Pflichtfeld). Der **Renderer** und die
   **Fill-Vorschau** verwenden ab jetzt die Objektfarbe.
2. Der **Layer** behält `ColorHex` als **Vorgabefarbe für neue Objekte** dieses
   Layers (bequemer Startwert, z. B. „neuer Schnitt-Layer → rote Objekte") sowie
   für die Anzeige im Layer-Panel. Er bestimmt weiterhin die **Laserparameter**
   (Modus, Speed, Power, Passes, Air Assist, …) — das bleibt seine eigentliche
   Aufgabe.
3. Beim **Zeichnen** eines neuen Objekts wird die aktuelle Layer-Vorgabefarbe in
   das Objekt kopiert.
4. Die **Farbpalette** (Design-Modus):
   - mit Auswahl → färbt die **selektierten Objekte** (ein Undo-Schritt),
   - ohne Auswahl → setzt die **Vorgabefarbe des aktiven Layers** (für die
     nächsten neuen Objekte).
5. Die **Fill-Vorschau** (ADR 0003 §5) bleibt: gefüllt wird weiterhin abhängig
   vom **Layer-Modus** (Fill/Raster) — nur die Farbe kommt jetzt vom Objekt.

## Konsequenzen

- Das Farb-Rendering wechselt von Layer- auf Objektfarbe; die Layer-Schleife im
  `CanvasControl` liest `obj.ColorHex` statt `layer.ColorHex`.
- Neuer Undo-Command `RecolorObjectsCommand` (mehrere Objekte, Vorher-/Nachher-
  Farbe) für das Umfärben der Auswahl.
- Bestehende Projekte ohne Objektfarbe erhalten beim Laden die Layerfarbe als
  Objektfarbe (Migration bei Bedarf; aktuell existieren keine gespeicherten
  Projekte).
- Der Farbchip im Layer-Panel zeigt weiter die Layer-Vorgabefarbe; er ist keine
  Objektfarbe mehr.

## Nicht Teil dieser Entscheidung

Farbverläufe, Alpha pro Objekt, oder das Binden von Laserparametern an einzelne
Objekte (Parameter bleiben Layer-Sache).
