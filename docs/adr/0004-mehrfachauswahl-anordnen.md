# ADR 0004: Mehrfachauswahl und Anordnen

## Status
Akzeptiert — 2026-07-06

## Kontext

[ADR 0003](0003-gui-gestaltung.md) §3 sieht eine **Anordnen-Palette**
(Ausrichten und Verteilen der Auswahl) vor. Ausrichten und besonders Verteilen
sind erst sinnvoll, wenn **mehrere Objekte gleichzeitig** ausgewählt sein können.

Der aktuelle Editor kennt nur Einzelauswahl: `MainWindowViewModel.SelectedObject`
(ein `CanvasObject?`), `CanvasControl` selektiert per Klick genau ein Objekt,
Auswahlrahmen/Handles/Verschieben und das Transform-Panel arbeiten auf diesem
einen Objekt. Mehrfachauswahl ist damit eine Erweiterung quer durch ViewModel,
Canvas-Control und Undo — deshalb dieses eigene ADR (CLAUDE.md Regel 10).

## Entscheidung

### 1. Auswahl ist eine Menge

Das ViewModel führt eine **Auswahlliste** (`ObservableCollection<CanvasObject>
SelectedObjects`). `SelectedObject` bleibt als **primäres** Objekt erhalten
(zuletzt hinzugefügt) und wird aus der Liste abgeleitet; bestehender Code, der
ein einzelnes Objekt braucht (z. B. Handles), nutzt weiter das primäre Objekt.

### 2. Interaktion

- **Klick** auf ein Objekt: exklusive Einzelauswahl.
- **Shift-/Strg-Klick**: Objekt zur Auswahl hinzufügen bzw. daraus entfernen
  (Toggle).
- **Rubber-Band** (mit dem Select-Werkzeug im Leeren aufziehen): alle Objekte,
  deren Bounding-Box vollständig im aufgezogenen Rechteck liegt, werden
  ausgewählt.
- **Klick ins Leere**: Auswahl aufheben.

### 3. Darstellung

- Jedes ausgewählte Objekt bekommt seinen Auswahlrahmen (rotationsgerecht wie
  in ADR 0003).
- **Größen-Handles nur bei genau einem, unrotierten Objekt** — wie bisher. Bei
  Mehrfachauswahl keine Handles; Größenänderung erfolgt über Palette/später.

### 4. Transform-Palette bei Mehrfachauswahl

Das Panel zeigt die **gemeinsame Bounding-Box** der Auswahl. Bei mehr als einem
Objekt sind zunächst nur **X/Y** (Verschieben der ganzen Gruppe) aktiv; Breite,
Höhe, Skalierung und Drehung bleiben der Einzelauswahl vorbehalten
(Gruppen-Skalierung/-Rotation ist ein späterer Schritt). So gibt es kein
irreführendes Feld, das bei Mehrfachauswahl das Falsche tut.

### 5. Anordnen (Ausrichten/Verteilen)

- **Fachlogik im Core** (`LuxiFer.Core`), ohne Avalonia, testbar: eine Funktion,
  die aus einer Objektmenge die neuen Positionen berechnet
  (Ausrichten: links/mitte/rechts/oben/mitte/unten; Verteilen: horizontal/
  vertikal mit gleichen Abständen).
- Ausrichten braucht **≥ 2** Objekte, Verteilen **≥ 3**; sonst sind die Aktionen
  deaktiviert.
- Jede Anordnen-Aktion ist **ein** Undo-Command über alle betroffenen Objekte
  (Positions-Deltas), analog zu den bestehenden Canvas-Commands.

## Konsequenzen

- `SelectedObject` wird zur abgeleiteten Sicht auf `SelectedObjects`;
  `CanvasControl` und das Transform-Panel werden entsprechend angepasst.
- Verschieben per Maus bewegt die **gesamte** Auswahl (ein Move-Command über
  alle Objekte).
- Die Anordnen-Buttons aus ADR 0003 §3 erhalten damit ihre Funktion.
- Rotation/Skalierung einer Mehrfachauswahl als Gruppe ist bewusst **nicht** Teil
  dieser Entscheidung und bleibt ein späterer Schritt.

## Nicht Teil dieser Entscheidung

Gruppen (dauerhaftes Zusammenfassen von Objekten), Ausrichten an Bezugsobjekt
oder Arbeitsraum, sowie Gruppen-Skalierung/-Rotation.
