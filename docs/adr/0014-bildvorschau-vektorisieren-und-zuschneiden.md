# ADR 0014: Bildvorschau, Vektorisieren und Zuschneiden

## Status

Umgesetzt — 2026-07-17. Bildparameter, Vektorisieren, rechteckiger Crop sowie
Kreis-/Ellipsen-Crop besitzen getrennte Live-Vorschauen. Abgeleitete Crop-
Assets werden referenzabhängig sichtbar gehalten und unreferenziert bereinigt.

Ergänzt und präzisiert
[ADR 0004](0004-asset-store-und-bild-import.md). Dessen Invarianten zum
zentralen Asset-Store, zur vollen Auflösung und zur nicht-destruktiven
Bildbearbeitung bleiben bestehen.

## Kontext

LuxiFer kann Bilder importieren, ihre Verarbeitungsparameter verändern und aus
ihnen Konturen erzeugen. Der aktuelle native Bilddialog macht das Ergebnis der
Regler jedoch nicht unmittelbar sichtbar und mischt zwei unterschiedliche
Arbeitsabsichten:

- ein vorhandenes Bild für Anzeige und Laser-Rasterung bearbeiten;
- aus einem Bild neue Vektorgeometrie erzeugen.

Dadurch werden Einstellungen derzeit weitgehend blind vorgenommen. Außerdem
fehlt ein Zuschneidewerkzeug. Vektorisieren und Zuschneiden benötigen jeweils
eine eigene Vorschau, eigene Parameter, einen abbrechbaren Entwurf und eine
eindeutige Aussage darüber, welche Projektdaten beim Anwenden entstehen.

ThorBurn dient als Verhaltensreferenz. Dort besitzt Vektorisieren einen eigenen
Vorschaudialog mit Trace-Parametern. Zuschneiden ist ein eigener Canvas-Modus
mit sichtbarer Maske, Griffen und mehreren Auswahlformen. LuxiFer übernimmt
diese sinnvollen Bediengrenzen, nicht jedoch ThorBurn-Code oder eine
destruktive Änderung der Asset-Datei.

## Entscheidung

LuxiFer behandelt **Bild bearbeiten**, **Vektorisieren** und **Zuschneiden** als
drei getrennte Aktionen. Sie dürfen gemeinsame Vorschau- und Bildpipeline-
Bausteine verwenden, besitzen aber getrennte UI-Zustände und getrennte
Application-Kommandos.

### 1. Gemeinsame Vorschaugrenze

Alle drei Werkzeuge verwenden dieselbe Core-eigene Bildverarbeitung wie Canvas
und Job-Aufbau. Die native Oberfläche zeigt lediglich deren Ergebnis.

Für jede Vorschau gelten folgende Regeln:

1. Das Asset im Store bleibt unverändert.
2. Änderungen werden zunächst nur in einem lokalen Dialog- oder
   Werkzeugentwurf gehalten.
3. Regleränderungen aktualisieren die Vorschau unmittelbar, ohne Undo-Schritt
   und ohne das Projekt als geändert zu markieren.
4. Teure Berechnungen dürfen im Hintergrund laufen. Ein älteres Ergebnis darf
   einen neueren Parameterstand nicht überschreiben.
5. `Anwenden` übergibt den vollständigen Entwurf genau einmal an die
   Application-Schicht und erzeugt genau einen Undo-Schritt.
6. `Abbrechen`, Escape und das Schließen des Dialogs verwerfen den Entwurf
   vollständig.
7. Preview-Caches sind ableitbar und werden nicht im Projekt gespeichert.

Die Vorschau besitzt mindestens `Einpassen`, Zoom und Verschieben. Bei großen
Bildern darf während einer Reglerbewegung eine kleinere Vorschau verwendet
werden; das angewandte Ergebnis und der Laserpfad rechnen weiterhin aus dem
Asset in voller Auflösung.

### 2. Bild bearbeiten per Doppelklick

Ein Doppelklick auf ein Bildobjekt öffnet primär den Bildeditor. Er startet
nicht die Vektorisierung und verändert nicht die aktuelle Canvas-Auswahl.

Der Editor zeigt eine große Vorschau sowie die bereits vorhandenen Parameter:

- Verarbeitungsmodus;
- Schwellenwert, wenn der Modus ihn benötigt;
- Helligkeit;
- Kontrast;
- Gamma;
- Invertierung für Canvas beziehungsweise Editor;
- getrennte Invertierung für den Laserpfad.

Die Vorschau kann zwischen **Editor** und **Laser** umgeschaltet werden. Beide
Ansichten nutzen denselben Parameterentwurf, unterscheiden sich aber bei der
dafür vorgesehenen Invertierung und der abschließenden Rasterdarstellung.

`Zurücksetzen` setzt nur den Entwurf auf die Standardwerte zurück. Erst
`Anwenden` speichert `ImageParams` am Bildobjekt. Das Bild-Asset und seine
Pixelbytes werden niemals überschrieben.

Vektorisieren und Zuschneiden erscheinen nicht als untergeordnete Abschnitte
dieses Dialogs. Sie sind separate Aktionen im Bild-Kontextmenü und in der
zugehörigen Werkzeuggruppe.

### 3. Eigenes Werkzeug „Vektorisieren“

Vektorisieren ist nur verfügbar, wenn genau ein Bildobjekt ausgewählt ist. Die
Aktion öffnet einen eigenen Dialog mit einer Vektorvorschau über dem Bild.

Der erste vollständige Funktionsumfang umfasst:

- Schwellenwert `0..255`;
- `Invertieren`;
- Glättung der Kontur;
- Detailgrad beziehungsweise Vereinfachung;
- Mindestfläche zum Entfernen kleiner Flecken;
- Ein-/Ausblenden des Quellbilds;
- Ein-/Ausblenden und farblich abgesetzte Darstellung der erzeugten Konturen;
- Zoom, Verschieben und `Einpassen`;
- sichtbare Ergebnisangaben wie Konturanzahl und verworfene Kleinstflächen.

Die Reihenfolge der fachlichen Verarbeitung ist stabil:

1. unverändertes Graustufen-Asset laden;
2. Helligkeit, Kontrast und Gamma des Bildobjekts anwenden;
3. Trace-Schwelle und Trace-Invertierung anwenden;
4. Konturen erkennen;
5. Mindestfläche filtern;
6. glätten und vereinfachen;
7. Pixelkoordinaten über die Bildbox in Millimeter umrechnen.

Die Vorschau und das endgültige Ergebnis müssen aus derselben Trace-Funktion
und demselben Parametersatz entstehen. Der bestehende Core-Trace wird dafür
erweitert; Konturerkennung oder Glättung gehören nicht in egui.

`Anwenden` erzeugt geschlossene Vektorpfade auf einem normalen Vektor-Layer und
wählt sie anschließend aus. Position, Größe und Rotation des Bildobjekts
werden in die neuen Pfade eingerechnet. Das Quellbild bleibt standardmäßig
erhalten und unverändert. Ein automatisches Löschen oder Ausblenden des Bildes
ist nicht Teil des ersten Schnitts.

Erzeugt der Parametersatz keine Kontur, bleibt `Anwenden` deaktiviert und die
Vorschau erklärt den Grund. Ein leeres Ergebnis erzeugt weder Shapes noch
Undo-Eintrag.

### 4. Eigenes Werkzeug „Zuschneiden“

Zuschneiden ist nur verfügbar, wenn genau ein Bildobjekt ausgewählt ist. Die
Aktion wechselt in einen abgegrenzten Canvas-Werkzeugmodus; sie öffnet keinen
weiteren allgemeinen Eigenschaften-Inspector.

Der erste Umsetzungsschnitt unterstützt einen **rechteckigen Ausschnitt**:

- Aufziehen eines neuen Ausschnitts auf dem Bild;
- sichtbare Abdunklung des verworfenen Bereichs;
- Verschieben des Ausschnitts innerhalb des Bildes;
- Größenänderung über acht Griffe;
- numerische Eingabe von Position und Größe;
- `Zurücksetzen` auf die vollständige Bildfläche;
- `Anwenden` beziehungsweise Enter;
- `Abbrechen` beziehungsweise Escape.

Der Ausschnitt darf die ursprüngliche Bildfläche nicht verlassen und darf
nicht auf Nullgröße kollabieren. Mauszeiger zeigen Erstellen, Verschieben und
die jeweilige Resize-Richtung an.

Der Crop wird **nicht-destruktiv** als normalisiertes Rechteck relativ zur
ungecroppten Bildfläche gespeichert:

```rust
struct ImageCrop {
    x: f64, // 0.0..1.0
    y: f64, // 0.0..1.0
    w: f64, // > 0.0, x + w <= 1.0
    h: f64, // > 0.0, y + h <= 1.0
}
```

Ein vollständiges Rechteck entspricht keinem wirksamen Crop und darf beim
Speichern als `None` normalisiert werden. Das Bildobjekt behält nach dem
Anwenden seine sichtbare Position und Rotation; seine Bounding-Box wird auf
den sichtbaren Ausschnitt verkleinert. Die effektive Pixel-zu-Millimeter-
Skalierung bleibt dabei erhalten: Zuschneiden vergrößert oder verzerrt das
Motiv nicht.

Canvas, Hit-Test, Auswahlbox, Editorvorschau, Vektorisierung, Thumbnail und
Laser-Rasterung berücksichtigen denselben Crop. Kein Verbraucher darf das
ungecroppt dargestellte Bild verwenden, nachdem ein Crop angewandt wurde.

Ellipse und freie Polygonform werden als zweiter Ausbau auf demselben
Werkzeugzustand vorgesehen. Sie erfordern eine allgemeine Crop-Maske statt des
rechteckigen `ImageCrop` und sind daher nicht Teil des ersten
Implementierungsschnitts. Automatische Motiverkennung beziehungsweise
`Auto-Zuschneiden` wird ebenfalls vertagt.

#### Umsetzungsstand 2026-07-17: interaktive Crop-Geometrie

Der zweite Ausbau hat mit dem elliptischen Crop begonnen. Der Crop-Dialog zeigt
das vollständige Bild als stabile Zeichenfläche. Rechtecke werden direkt in der
Vorschau aufgezogen und anschließend an ihren Eckgriffen verändert. Ein Kreis
entsteht als Umkreis durch drei frei gesetzte Umfangspunkte. Nach der
Konstruktion erhält er eine achsenparallele Bounding Box mit acht Griffen; über
diese kann der Kreis anschließend auch zu einer Ellipse verzerrt werden.

Beim Anwenden wird die Ellipse in ein abgeleitetes Rasterasset mit echtem
Alpha-Kanal maskiert; Pixel außerhalb der Ellipse sind transparent und nicht
weiß gefüllt. Das Ergebnis wird auf seine achsenparallele Begrenzung
zugeschnitten. Die Application übergibt diese
Begrenzung weiter an den vorhandenen atomaren Session-Crop, sodass Assetwechsel,
Bildbox-Anpassung und Undo weiterhin ein gemeinsamer Schritt bleiben. Das
Originalasset wird nicht verändert. Freie Polygonmasken bleiben offen.

Crop-Ergebnisse sind im Asset-Metadatensatz als `derived` markiert. Sie werden
im Asset-Katalog nur angezeigt, wenn mindestens eine gespeicherte
Projektversion ihre ID referenziert. Dadurch tauchen verworfene oder nur in der
Undo-Historie gehaltene Zwischenstände nicht als normale Bibliotheksassets auf.
Beim Programmstart werden abgeleitete Assets ohne Referenz aus irgendeiner
gespeicherten Projektversion physisch entfernt. Importierte Originale sind von
dieser Bereinigung grundsätzlich ausgeschlossen. Der Legacy-Name
`Bildausschnitt.png` wird für bereits vor Einführung des Markers erzeugte Crops
einmalig wie ein abgeleitetes Asset behandelt.

### 5. Schichten und Zuständigkeiten

- `luxifer-core` besitzt Crop-Datenmodell, Validierung, effektive Bildregion,
  Trace-Parameter und sämtliche Pixel-/Konturberechnung.
- `luxifer-application` prüft Auswahl und Objekttyp und wendet Crop,
  Bildparameter oder Trace-Ergebnis jeweils atomar mit genau einem
  Undo-Schritt an.
- `luxifer-native` besitzt Dialogentwürfe, Werkzeugzustand, Preview-Texturen
  und Eingabegesten. Es mutiert keine Core-Strukturen direkt.
- Maschinen- und Ruida-Treiber kennen weder Crop-Dialog noch Trace-Parameter;
  sie erhalten ausschließlich den bereits kompilierten JobPlan.

## Invarianten

1. Quell- und Store-Asset werden durch keines der drei Werkzeuge verändert.
2. Vorschau und angewandtes Ergebnis verwenden dieselbe Core-Funktion und
   denselben Parametersatz.
3. Abbrechen hinterlässt weder Projektmutation noch Undo-Eintrag oder neue
   Asset-Datei.
4. Jede bestätigte Aktion ist genau ein Undo-Schritt.
5. Vektorisieren erzeugt neue Vektorgeometrie; es wandelt das Bildobjekt nicht
   in-place in einen anderen Typ um.
6. Zuschneiden erzeugt ein abgeleitetes, per Metadatum markiertes Asset; das
   Original bleibt unverändert und unreferenzierte Ableitungen werden bereinigt.
7. Bildbearbeitung, Vektorisieren und Zuschneiden bleiben getrennte
   Application-Aktionen, auch wenn sie UI-Bausteine teilen.

## Nicht Teil dieser Entscheidung

- konturbasiertes Nesting; dafür folgt ein eigenes ADR;
- Löschen oder automatisches Ersetzen des Quellbilds nach dem Vektorisieren;
- manuelles Zeichnen und Editieren einzelner Trace-Knoten im Vorschaudialog;
- Mehrfarb- oder Mehrschwellenwert-Vektorisierung;
- OCR oder Erkennung semantischer Bildinhalte;
- perspektivische Entzerrung und freie Bildtransformation;
- destruktives Exportieren eines Crops als neues Asset;
- freie Polygon-Crop-Masken;
- automatische Motiv- beziehungsweise Rand-Erkennung im ersten Schnitt.

## Abnahmekriterien

Die Entscheidung gilt als umgesetzt, wenn:

- ein Doppelklick auf ein Bild den Bildeditor mit sichtbarer Editor- und
  Laservorschau öffnet;
- jede Regleränderung die Vorschau aktualisiert, ohne vor `Anwenden` das Projekt
  zu verändern;
- Vektorisieren ein eigener Dialog ist und seine endgültigen Pfade pixelgleich
  zur dargestellten Trace-Vorschau ableitet;
- Mindestfläche, Glättung und Vereinfachung durch Core-Tests abgedeckt sind;
- ein rechteckiger Crop mit Maus und numerisch bearbeitet werden kann;
- Crop-Geometrie nach Speichern und erneutem Öffnen identisch bleibt;
- ein gecropptes Bild in Canvas, Hit-Test, Auswahlbox, Trace und Laser-Job
  denselben sichtbaren Ausschnitt verwendet;
- Anwenden jeweils genau einen Undo-Schritt erzeugt und Undo den vollständigen
  vorherigen Zustand wiederherstellt;
- Abbrechen und leere Trace-Ergebnisse keine Mutation hinterlassen;
- große Bildassets die UI während der Vorschauerzeugung nicht blockieren.

## Umsetzungsreihenfolge

1. Gemeinsamen Core-Preview-Auftrag und generationengesicherten nativen
   Preview-Cache einführen.
2. Den bestehenden Bilddialog auf große Live-Vorschau und reinen
   Bildparameter-Entwurf begrenzen.
3. Trace-Parameter im Core vervollständigen und den separaten
   Vektorisierungsdialog darauf aufbauen.
4. `ImageCrop` ins Projektmodell aufnehmen und alle Bildverbraucher auf eine
   gemeinsame effektive Bildregion umstellen.
5. Rechteckigen Crop-Werkzeugmodus mit Overlay, Griffen und numerischer Eingabe
   ergänzen.
6. Workspace-Tests, Clippy sowie manuelle Prüfung mit S/W-Illustration,
   gedrehtem Bild, großem Asset und Laser-Vorschau durchführen.

## Konsequenzen

### Positiv

- Bildparameter werden nicht länger blind eingestellt.
- Die drei unterschiedlichen Arbeitsabsichten sind in der Oberfläche klar
  getrennt.
- Vorschau, gespeicherter Zustand und Laserresultat können nicht durch
  getrennte Algorithmen auseinanderlaufen.
- Originalassets bleiben erhalten, und Crop-Varianten vervielfachen nicht den
  Asset-Store.
- Die fachlichen Funktionen bleiben unabhängig von egui testbar.

### Kosten und Risiken

- Ein Crop betrifft jeden Verbraucher von `Geo::Image`; eine nur visuelle
  Canvas-Lösung wäre unvollständig und ist ausdrücklich ausgeschlossen.
- Generationengesicherte Hintergrundvorschauen benötigen einen klaren
  Lebenszyklus für CPU-Daten und GPU-Texturen.
- Glättung kann Details verlieren und Vereinfachung Konturen verändern. Deshalb
  sind getrennte Regler, unmittelbare Vorschau und stabile Standardwerte
  erforderlich.
- Elliptische und freie Crops benötigen später ein allgemeineres Maskenmodell
  und dürfen nicht als Sonderfälle in das Rechteckmodell gezwängt werden.
