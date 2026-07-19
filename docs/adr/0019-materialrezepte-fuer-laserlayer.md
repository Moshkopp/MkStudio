# ADR 0019: Materialrezepte für Laser-Layer

- Status: Experimentell implementiert
- Datum: 2026-07-18
- Betrifft: Layerparameter, Laserprofile, Materialbibliothek, Settings

## Ausgangspunkt

LuxiFer soll häufig verwendete Kombinationen aus Material, Prozess und
Laserparametern wiederverwenden können. Ein Nutzer soll beispielsweise nicht
jedes Mal Geschwindigkeit, Leistung und Durchläufe für „Pappelsperrholz 3 mm
schneiden“ neu eingeben müssen.

Die Vorgängerversionen lösten dies als `Layer-Templates`. Die Referenzdaten
zeigen dabei mehrere strukturelle Schwächen:

- eine starre Hierarchie `Kategorie → Material → Stärke → Cut/Fill`;
- Werte wurden teilweise als Strings gespeichert;
- jedes Template enthielt auch für seinen Prozess irrelevante Felder;
- die Eignung für eine konkrete Maschine war nicht eindeutig;
- Template, Materialwissen und fertige Layerparameter waren vermischt;
- die Oberfläche musste durch mehrere Ebenen navigiert werden;
- Änderungen und Herkunft angewendeter Werte waren nicht nachvollziehbar.

Das eigentliche Problem ist daher nicht „Layerwerte speichern“, sondern:

> Wie wird aus erprobten Parametern ein auffindbares, maschinenbezogenes und
> trotzdem unkompliziert anwendbares Rezept, ohne eine falsche
> Erfolgsgarantie zu vermitteln?

## Aktuelle Versuchsrichtung

Die folgenden Punkte ersetzen für den Prototyp die weiter unten noch
dokumentierten, umfangreicheren Bibliotheksideen, soweit sie ihnen
widersprechen. Sie bleiben zunächst im ADR stehen, damit nachvollziehbar ist,
welche Ansätze wir bewusst vereinfachen.

- Material ist **kein Projektmerkmal**. Zwei Nutzer dürfen dasselbe Design auf
  unterschiedlichen Lasern und Materialien verwenden.
- Der obere Header enthält zwischen Laser-Vorschau und Projektname nur
  Laserwahl und Verbinden/Trennen.
- Pro Laser gibt es kleine Materialprofile mit höchstens je einem optionalen
  Standard für Schneiden, Vektorgravur und Raster/Bildgravur.
- Das Design wird zuerst vollständig erstellt. Materialwerte werden nicht beim
  Zeichnen automatisch verteilt.
- „Layer verwalten“ oberhalb der Layerliste öffnet einen gemeinsamen Entwurf
  aller vorhandenen Layer.
- Materialauswahl und „Materialwerte laden“ leben ausschließlich in diesem
  Dialog. Laden aktualisiert sichtbar die Tabelle; manuelle Anpassungen bleiben
  direkt möglich.
- Erst „Speichern“ übernimmt alle Tabellenzeilen atomar in einem Undo-Schritt;
  „Abbrechen“ verwirft den gesamten Entwurf.
- Die umfangreiche manuelle Template-Historie entfällt. Stattdessen soll ein
  späterer Laserjob automatisch einen unveränderlichen Journal-Snapshot
  erzeugen; eine Bewertung bleibt optional.
- Das Projektmanagement bewahrt konkrete Projektversionen. Das Laserjournal
  beantwortet ergänzend, welche Werte material- und maschinenübergreifend
  tatsächlich verwendet und als gut bewertet wurden.

Der Grundablauf ist implementiert. Journal und Bewertung folgen erst, wenn er
sich praktisch bewährt.

Die Schichtengrenzen sind verbindlich: Der Core besitzt und validiert das
UI-unabhängige Materialmodell. `MaterialService` in der Application besitzt die
Anwendungsfälle, Prozesszuordnung und persistente Bibliothek. Native zeigt und
koordiniert diese Anwendungsfälle, enthält aber weder JSON-Dateizugriffe noch
eine zweite Parameterzuordnung. Bibliotheksänderungen werden zuerst in einer
Kopie aufgebaut und per temporärer Datei ersetzt; erst nach erfolgreichem
Schreiben wird der laufende Zustand aktualisiert. Eine beschädigte Datei wird
als Fehler gemeldet und niemals still durch eine leere Bibliothek ersetzt.

## Begriffe

### Material

Beschreibt das Werkstück, nicht die Maschine:

- Werkstoff, z. B. Pappelsperrholz, MDF, Schiefer oder Acryl;
- optionale Variante, z. B. Hersteller, Farbe oder Beschichtung;
- Stärke in Millimetern, wenn relevant;
- freie Notizen und optionale Tags.

### Prozess

Beschreibt das Ziel der Bearbeitung:

- Kontur schneiden;
- Vektorfläche gravieren;
- Raster/Bild gravieren.

Die Prozessarten orientieren sich an den heutigen `LayerMode`-Varianten, werden
aber benutzerverständlich benannt. Ein Bildrezept und ein Vektorgravurrezept
können unterschiedliche relevante Parameter besitzen.

### Materialrezept

Ein Materialrezept verbindet Material, Prozess, konkrete Parameter und den
Gültigkeitsbereich. Es ist ein dokumentierter Erfahrungswert, keine
Materialeigenschaft und keine Garantie für ein Ergebnis.

Der Begriff „Template“ wird in der UI vermieden, weil LuxiFer bereits
Design-/Projektvorlagen kennt und ein Laserrezept semantisch etwas anderes ist.

## Leitideen für die Diskussion

### 1. Rezepte sind maschinenbezogen

Geschwindigkeit und Leistung sind ohne Maschine nicht zuverlässig
übertragbar. Laserleistung, Optik, Fokus, Luftzufuhr, Mechanik und Zustand des
Materials verändern das Ergebnis.

Erste Empfehlung:

- ein selbst angelegtes Rezept gehört standardmäßig zum konkreten
  `LaserProfile`;
- es darf bewusst dupliziert und einem anderen Profil zugeordnet werden;
- LuxiFer überträgt Werte nie still auf eine andere Maschine;
- ein optionaler Gültigkeitsbereich „nicht zugeordnet“ dient nur importierten
  oder noch nicht getesteten Startwerten und wird sichtbar gewarnt.

Damit bleiben Laserprofile weiterhin reine Geräte-/Kalibrierdaten. Die Rezepte
liegen in einer eigenen globalen Bibliothek und referenzieren nur die stabile
Profil-ID.

### 2. Anwenden erzeugt einen Snapshot

Ein Layer muss auch dann vollständig und reproduzierbar bleiben, wenn ein
Rezept später geändert oder gelöscht wird. Deshalb kopiert „Anwenden“ die
Parameter in den Layer. Es entsteht keine dauerhafte Live-Verknüpfung.

Zusätzlich kann der Layer optionale Herkunftsdaten tragen:

- Rezept-ID und Rezeptrevision;
- Anzeigename zum Zeitpunkt der Anwendung;
- Zeitpunkt der Anwendung;
- Kennzeichen, ob relevante Werte danach manuell verändert wurden.

Die Herkunft ist Information, nicht Autorität. Eine spätere Rezeptänderung
überschreibt bestehende Projekte niemals automatisch. Ein explizites
„Mit aktueller Rezeptversion vergleichen“ kann später Abweichungen anzeigen.

### 3. Nur relevante Parameter pro Prozess

Gemeinsame Parameter:

- Geschwindigkeit in `mm/s`;
- maximale und minimale Leistung in Prozent;
- Durchläufe;
- Air Assist.

Zusätzliche Parameter nach Prozess:

| Prozess | Zusätzliche Werte |
|---|---|
| Schneiden | zunächst keine |
| Vektorgravur | Linienabstand |
| Raster/Bild | DPI, bidirektional |

Ein Rezept speichert keine bedeutungslosen Platzhalter. Das Core-Modell soll
die Prozessvariante typisiert abbilden, statt eine große Struktur mit Feldern
für alle Fälle zu verwenden.

Noch zu klären ist, ob minimale Leistung bei allen Treibern dieselbe fachliche
Bedeutung besitzt. Wo ein Treiber einen Wert nicht unterstützt, muss die
Capability-Grenze ihn sichtbar deaktivieren oder beim Anwenden ablehnen; er
darf nicht still ignoriert werden.

### 4. Flache, durchsuchbare Bibliothek statt tiefem Auswahlbaum

Die Hauptansicht der Rezeptbibliothek ist eine filterbare Tabelle oder
Kartenliste. Sinnvolle sichtbare Spalten:

- Material und Variante;
- Stärke;
- Prozess;
- zugeordneter Laser;
- Speed, Power und Durchläufe;
- Status „getestet“ oder „Startwert“;
- letzte Änderung.

Filterchips für Laser, Prozess, Material und Stärke ersetzen die Navigation
durch mehrere verschachtelte Ebenen. Gruppieren nach Material bleibt eine
Ansichtsoption, ist aber nicht Teil der Datenidentität.

### 5. Zwei natürliche Wege zum Rezept

#### Aus einem funktionierenden Layer speichern

Im Layerdialog bietet „Als Materialrezept speichern …“ die aktuell erprobten
Werte an. Der Nutzer ergänzt Material, Stärke, Variante und Notiz. Dieser Weg
ist wahrscheinlich der häufigste, weil er von einem realen Ergebnis ausgeht.

#### In der Bibliothek anlegen und pflegen

Eine eigene Verwaltung ermöglicht Anlegen, Duplizieren, Umbenennen,
Bearbeiten, Archivieren, Löschen, Import und Export. Die Verwaltung könnte
entweder ein eigener Bereich in den Settings oder ein eigener Dialog aus dem
Laserpanel sein. Da Rezepte Arbeitsdaten und keine Softwareoptionen sind,
tendiert dieser Entwurf zu einem eigenen Bibliotheksdialog.

### 6. Anwendung im Layerdialog

Oben im Layerdialog erscheint ein klar abgesetzter Bereich „Materialrezept“:

1. standardmäßig werden passende Rezepte für aktiven Laser und aktuellen
   Prozess gezeigt;
2. Suche und Filter erlauben Material und Stärke;
3. vor dem Anwenden werden die abweichenden Werte kompakt gegenübergestellt;
4. „Werte anwenden“ kopiert die Parameter in den Dialogentwurf;
5. erst das normale „Speichern“ übernimmt den Layerentwurf in das Projekt.

Damit bleibt die bestehende Abbrechen-Semantik erhalten. Die Rezeptauswahl
mutiert den Layer nicht sofort.

### 7. Teststatus statt falscher Sicherheit

Jedes Rezept besitzt einen Status:

- `Startwert`: importiert, übertragen oder noch nicht bestätigt;
- `Getestet`: vom Nutzer auf dem zugeordneten Laser als brauchbar markiert;
- optional später `Favorit` als reine Organisation.

„Getestet“ bedeutet nur eine dokumentierte Nutzeraussage. Die UI zeigt stets
einen kurzen Sicherheitshinweis: Materialcharge, Fokus, Optik und Maschine
können abweichen; zuerst Probeschnitt beziehungsweise Testgravur ausführen.

Eingebaute universelle Leistungswerte werden zunächst nicht mitgeliefert. Sie
würden eine Genauigkeit vortäuschen, die LuxiFer ohne Kenntnis der Maschine
nicht besitzt. Denkbar sind später importierbare Hersteller-/Community-Pakete,
die immer als ungeprüfte Startwerte gekennzeichnet sind.

### 8. Lebenszyklus und Datensicherheit

Erste Empfehlung:

- eigene versionierte JSON-Datei im App-Datenverzeichnis;
- stabile UUID pro Rezept und monoton steigende Rezeptrevision;
- atomisches Speichern;
- Einbeziehung in Charon-Backup/Restore;
- Export einzelner Rezepte oder eines Bundles;
- Import zeigt Dubletten und Profilzuordnung vor dem Schreiben;
- Löschen eines verwendeten Rezepts beschädigt kein Projekt, da Layer Snapshots
  enthalten;
- zunächst Archivieren als sichere Standardaktion, endgültiges Löschen nur
  bewusst.

Ein Import übernimmt keine fremde Laserprofil-ID automatisch. Der Nutzer muss
das Zielprofil wählen oder das Rezept als ungeprüften Startwert importieren.

### 9. Laser im Header, Material im Layer-Manager

Der globale Header erhält ausschließlich den maschinenbezogenen Kontext:

- Auswahl des aktiven Laserprofils zwischen Laser-Vorschau und Projektname;
- kompakter sichtbarer Verbindungszustand mit Verbinden/Trennen.

Die Laser-Verwaltung bleibt der Ort zum Anlegen und Kalibrieren von Geräten.
Der Header dient nur dem häufigen Auswählen und Verbinden. Das Laser-Tab bleibt
Auswahl-/Joboberfläche und ist nicht länger Voraussetzung für den
Maschinenwechsel. Material ist dagegen kein allgemeiner Headerzustand. Es wird
erst nach abgeschlossenem Design im Layer-Manager gewählt und sichtbar auf die
konkreten Tabellenzeilen angewandt. So gibt es keine versteckte Veränderung
beim Zeichnen oder beim Erzeugen neuer Farblayer.

### 10. Spätere Idee: gute manuelle Werte ins Rezept zurückführen

Ein aus einem Rezept erzeugter Layer kann anschließend manuell verbessert
werden. Sobald relevante Parameter abweichen, zeigt der Layerdialog den Status
„Vom Rezept abweichend“ und bietet die Aktion:

> Änderungen ins Rezept übernehmen …

Ein Klick öffnet einen kleinen Bestätigungsdialog mit der Differenz zwischen
Rezeptrevision und aktuellen Layerwerten. Der Nutzer kann:

- das bestehende Rezept als neue Revision aktualisieren;
- unter neuem Namen ein eigenes Rezept daraus erstellen;
- abbrechen.

Das Aktualisieren ist bewusst kein völlig stiller Ein-Klick-Schreibvorgang,
weil Speed und Power sicherheitsrelevant sind. Der Dialog soll aber so kompakt
sein, dass Prüfen und Bestätigen der Normalfall bleiben.

Eine neue Rezeptrevision verändert andere Layer und ältere Projekte nicht
automatisch. Optional kann direkt danach „Passende Layer in diesem Projekt
aktualisieren …“ gewählt werden. Dabei werden nur Layer automatisch
vorausgewählt, die noch exakt auf der alten Rezeptrevision stehen. Ebenfalls
manuell abweichende Layer benötigen eine eigene Bestätigung.

Beim Übernehmen kann das Rezept zugleich als „getestet“ markiert werden. Eine
kurze Ergebnisnotiz ist optional; LuxiFer verlangt zunächst kein umfangreiches
Versuchsprotokoll. So entsteht der Lernkreislauf:

`Rezept anwenden → real testen → Layer feinjustieren → Rezept verbessern`.

## Möglicher Core-Schnitt

Noch keine endgültige API, sondern ein Diskussionsmodell:

```rust
struct MaterialRecipe {
    id: RecipeId,
    revision: u32,
    name: String,
    material: MaterialSpec,
    process: RecipeProcess,
    laser_profile_id: Option<LaserId>,
    parameters: RecipeParameters,
    confidence: RecipeConfidence,
    notes: String,
    tags: Vec<String>,
}

struct MaterialSpec {
    material: String,
    variant: Option<String>,
    thickness_mm: Option<f64>,
}

enum RecipeParameters {
    Cut(CommonLaserParameters),
    VectorEngrave {
        common: CommonLaserParameters,
        line_step_mm: f64,
    },
    RasterEngrave {
        common: CommonLaserParameters,
        dpi: f64,
        bidirectional: bool,
    },
}
```

Ob `RecipeProcess` neben `RecipeParameters` nötig ist oder redundant wäre,
wird vor der Umsetzung entschieden. Ebenso muss geklärt werden, ob
`MaterialSpec` später als eigene deduplizierte Entität oder zunächst als
eingebetteter Wert geführt wird.

## Bewusst noch offene Entscheidungen

1. Sollen Rezepte ausschließlich einem konkreten Laserprofil gehören, oder
   brauchen wir zusätzlich bewusst „unzugeordnete Startwerte“?
2. Soll „getestet“ nur ein Status sein oder auch Datum, Fokus/Optik und eine
   kurze Ergebnisnotiz erfassen?
3. Ist die Rezeptverwaltung ein eigener globaler Dialog oder ein Bereich der
   Laser-Verwaltung? Settings werden derzeit bewusst von Gerätedaten getrennt.
4. Soll Stärke ein freier optionaler Millimeterwert sein oder zusätzlich eine
   gut lesbare Anzeige wie `3 mm` erlauben? Der gespeicherte Wert sollte
   jedenfalls numerisch bleiben.
5. Werden Rezeptänderungen als echte Historie aufbewahrt oder genügt zunächst
   die aktuelle Revision plus Herkunftsrevision im Layer?
6. Brauchen wir früh einen Material-Testgenerator, der Speed/Power-Matrizen
   erzeugt und das beste Feld direkt als Rezept speichert?
7. Soll ein Rezept auf mehrere ausgewählte Layer gleichzeitig angewendet werden
   können, sofern deren Prozess kompatibel ist?

## Nicht Teil der ersten Ausbaustufe

- automatische Empfehlung durch KI oder Materialerkennung;
- automatische Umrechnung zwischen Lasern unterschiedlicher Leistung;
- ungeprüfte Cloud-/Community-Synchronisation;
- automatische Aktualisierung bereits gespeicherter Projektlayer;
- Garantie eines sicheren oder vollständigen Schnitts;
- Materiallager, Bestände und Einkauf.

## Vorgeschlagene nächste Verfeinerung

Vor einer Implementierungsentscheidung gehen wir drei konkrete Abläufe durch:

1. Ein Nutzer hat durch Probieren gute Layerwerte gefunden und speichert sie.
2. Ein Nutzer öffnet später ein anderes Projekt und wendet das Rezept an.
3. Ein zweiter Laser kommt hinzu und ein bestehendes Rezept soll übernommen
   und neu getestet werden.

Erst wenn diese Abläufe ohne Sonderregeln verständlich funktionieren, werden
Datenmodell, Speicherort und UI verbindlich entschieden.

## Erster Prototypstand 2026-07-18 – verworfen, uncommitted

Die erste testbare Scheibe ist umgesetzt, bleibt aber ausdrücklich
uncommitted:

- zweite Headerzeile „Arbeitskontext“ mit Laserwahl, Verbinden/Trennen und
  Materialwahl;
- eigene lokale Datei `material-profile.json`, nicht Teil des Projekts;
- Materialprofile sind an eine konkrete Laser-ID gebunden;
- je Material optionale Standards für Schneiden, Vektorgravur und
  Raster/Bildgravur;
- Material anlegen, bearbeiten und löschen direkt aus dem Header;
- Zeichnen sowie Bild-/SVG-/DXF-Import übernehmen passende Werte nur bei neu
  erzeugten Layern;
- bestehende Farblayer werden nie still verändert;
- Materialwechsel bei vorhandenen Layern zeigt eine Vergleichstabelle und
  bietet „Nur für neue Layer“ oder die selektive Umstellung;
- die Sammelumstellung ist ein Undo-Schritt;
- der Layerdialog kann gute manuelle Werte mit „Speichern +
  Materialstandard aktualisieren“ zurückführen.

Das automatische Laserjournal und die Ergebnisbewertung sind noch nicht
implementiert. Diese zweite Scheibe folgt erst nach praktischem Feedback zum
Grundablauf. Der vollständige Workspace-Test ist mit 255 Core-, 85
Application-, 54 Native-, 25 Ruida-, 5 GRBL- und 8 Charon-Tests grün.

### Erste Workflow-Korrektur

Der erste Prototyp übernahm beim Erzeugen eines Farblayers nur den
Schneidstandard, weil neue Layer technisch zunächst im Modus `Cut` entstehen.
Ein anschließender Wechsel auf `Fill` ließ diese Werte fälschlich stehen. Der
Layerdialog zeigt nun das aktive Material, lädt bei einem Moduswechsel
automatisch den passenden Prozessstandard und bietet zusätzlich jederzeit
„Materialstandard anwenden“. Damit ist `Layer anlegen → Fill wählen → passende
Gravurwerte erhalten` ein durchgehender Ablauf.

### Zweiter Prototyp: Design zuerst, Layer gemeinsam konfigurieren

Der erste Ansatz blieb trotz Moduskorrektur ohne überzeugenden Gesamtworkflow:
Materialauswahl und automatische Werteverteilung geschahen zu früh während des
Designens. Er wurde vollständig aus den Laufzeitpfaden entfernt.

Der zweite Prototyp setzt den bestätigten Ablauf um:

1. Design mit allen Layern fertigstellen.
2. „Layer verwalten“ oberhalb der Layerliste öffnen.
3. Aktiven Laser sehen und Material im Dialog auswählen.
4. „Materialwerte laden“ verteilt passende Schneid-, Vektorgravur- und
   Rasterwerte in den Tabellenentwurf.
5. Namen, Prozess, Speed, Min-/Max-Power, Durchläufe, Air Assist und
   Prozessdetails direkt zeilenweise anpassen.
6. „Speichern“ validiert erst alle Zeilen und übernimmt sie gemeinsam in einem
   Undo-Schritt; „Abbrechen“ verändert nichts.

Materialprofile lassen sich aus dem Layer-Manager anlegen, bearbeiten und
löschen. Die Laserwahl sowie Verbinden/Trennen stehen kompakt in der oberen
Headerzeile zwischen Hauptnavigation und Projektname. Material bleibt aus dem
Header und aus dem Projektformat heraus.

Der vollständige Workspace-Test ist mit 255 Core-, 87 Application-, 55
Native-, 25 Ruida-, 5 GRBL- und 8 Charon-Tests grün. Der gesamte zweite
Prototyp bleibt uncommitted.

Die Fensterhöhe ist an die verfügbare Bildschirmfläche gebunden. Kopfbereich
und Speichern/Abbrechen bleiben fest sichtbar; ausschließlich die Layertabelle
scrollt vertikal beziehungsweise horizontal. Damit bleibt der Dialog auch bei
kleineren Fenstern vollständig bedienbar.
