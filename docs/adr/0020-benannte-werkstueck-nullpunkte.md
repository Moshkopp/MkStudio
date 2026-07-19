# ADR 0020: Benannte Werkstück-Nullpunkte pro Laser

- Status: Vorgeschlagen
- Datum: 2026-07-19
- Betrifft: Laserpanel, Laserprofile, Positionsanzeige, Maschinenbewegung,
  Job-Startreferenz

## Kontext

Auf dem Laserbett soll eine Halterung dauerhaft an derselben Stelle montiert
werden können. Der Nutzer fährt den Laserkopf einmal an den Bezugspunkt der
Halterung, speichert diese Position beispielsweise als „Untersetzer Posi“ und
kann sie bei späteren Arbeiten wieder anfahren oder als Bezugspunkt eines Jobs
verwenden. Das wiederholte manuelle Ausrichten entfällt.

Die frühere Oberfläche zeigte bereits

- die aktuelle absolute Kopfposition und
- die absolute Position des controllerseitigen Benutzerursprungs.

Diese Anzeigen fehlen im neuen nativen Laserpanel. Das ist eine Regression der
Native-Migration und keine neue Fachfunktion. Die technische Grundlage ist
teilweise vorhanden: `MachineDriver::status()` liefert eine `MachineStatus` mit
Kopfposition, `MachineDriver::read_origin()` liest den Benutzerursprung und der
Ruida-Treiber implementiert beide Aufrufe. Application und Native reichen die
Werte derzeit jedoch nicht bis zur Oberfläche durch.

Der vorhandene `StartMode::Benutzerursprung` bezeichnet genau einen im
Controller gespeicherten Ursprung. Er eignet sich nicht als Modell für mehrere
frei benannte Positionen. Benannte Werkstück-Nullpunkte sind app-seitige Daten
und dürfen außerdem nicht zwischen Maschinen vermischt werden.

## Entscheidung

**(A) Die Positionsanzeige der Vorgängerversion wird im nativen Laserpanel
wiederhergestellt und folgt der gewählten Startreferenz. (B) Der zugehörige
Ursprung wird an seiner realen Position im Canvas eingezeichnet. (C) Studio
verwaltet zusätzlich mehrere benannte Werkstück-Nullpunkte pro Laserprofil.
(D) Auswahl, Anfahren und Verwendung als Jobreferenz bleiben getrennte,
ausdrückliche Aktionen.**

### A. Positionsanzeige folgt der gewählten Startreferenz

Die Positionsanzeige erfolgt visuell über die Fadenkreuze im Laser-Canvas
(siehe B): **Kopf** zeigt die live gelesene absolute Kopfposition, **Ursprung**
die Referenzkoordinate der Auswahl unter „Starten von“. Ein zusätzlicher
Textblock im Panel entfällt bewusst, damit das Panel ruhig bleibt
(Bedienentscheid während der Umsetzung); die exakten Zahlenwerte erscheinen im
Speichern-Dialog und in der Nullpunktverwaltung. Für die Referenzkoordinate
gilt verbindlich:

| Auswahl | Quelle der Referenzkoordinate | Anzeige als Ursprung |
|---|---|---|
| Absolute Koordinaten | Maschinenursprung 0/0 | ja |
| Aktuelle Position | live gelesene Kopfposition | ja |
| Benutzerursprung | am Ruida-Hardwarepanel gesetzter Benutzerursprung | ja |
| Gespeicherter Nullpunkt | X/Y des gewählten Profileintrags | ja |

Der Benutzerursprung wird somit nur gelesen und angezeigt, wenn
`Benutzerursprung` angewählt ist. Er ist der am Ruida-Controller beziehungsweise
dessen Hardwarepanel eingestellte Nullpunkt; Studio setzt oder verschiebt ihn
nicht. Bei `Aktuelle Position` ist der Ursprung die live gelesene Kopfposition.
Bei einem gespeicherten Nullpunkt ist er dessen gespeicherte absolute
Koordinate. Bei `Absolute Koordinaten` liegt der angezeigte Ursprung am
Maschinenursprung 0/0. Dadurch bleibt die Bedeutung des Fadenkreuzes bei jeder
Auswahl gleich; lediglich seine Koordinate wechselt.

Die Application stellt dafür UI-unabhängige Anwendungsfälle zum Lesen von
`status()` und `read_origin()` bereit. Native hält nur den zuletzt erfolgreich
gelesenen Anzeigestand und zeigt Fehler beziehungsweise einen unbekannten Wert
sichtbar an. Sie erfindet keine Position und behält nach einem Fehler keinen
veralteten Wert ohne Kennzeichnung bei.

Die Kopfposition und eine davon abhängige Referenz werden nach dem Verbinden,
nach Jog/Home/Anfahren und während einer aktiven Verbindung in einem
angemessenen Intervall aktualisiert. Der Benutzerursprung wird beim Anwählen
und danach bei einer gezielten Aktualisierung gelesen, nicht ohne Bedarf in
jeder Ansicht abgefragt. Das Polling darf weder das UI blockieren noch laufende
Maschinenbefehle unkontrolliert überholen. Treiber ohne die jeweilige
Lesefähigkeit zeigen „nicht unterstützt“ statt erfundener Koordinaten.

Die Anzeige verwendet die vom Treiber gelieferten absoluten
Maschinenkoordinaten. Die im Profil konfigurierte Lage des Maschinen-Nullpunkts
(`BedOrigin`) ist eine Abbildung für Bett- und Jobgeometrie und darf die live
gelesenen Registerwerte nicht ein zweites Mal spiegeln.

### B. Start, Ursprung und Kopf als Fadenkreuze anzeigen

Der Laser-Canvas zeichnet drei fachlich getrennte Fadenkreuze. Jedes Kreuz hat
eine dauerhaft zugeordnete, gut sichtbare Farbe und eine Textbeschriftung:

| Fadenkreuz | Bedeutung | Farbe | Position der Beschriftung |
|---|---|---|---|
| **Start** | gewählter 3×3-Jobanker auf der Job-BBox der aktiven Inhalte | Grün | links oberhalb des Kreuzes |
| **Ursprung** | Bezugspunkt der gewählten Startreferenz | Blau | rechts oberhalb des Kreuzes |
| **Kopf** | live gelesene Position des Laserkopfs | Orange | rechts unterhalb des Kreuzes |

Die Farben sind nicht die einzige Unterscheidung: Jedes Fadenkreuz trägt
immer seine Bezeichnung **„Start“**, **„Ursprung“** oder **„Kopf“**. Kontrast,
Linienstärke und Textkontur beziehungsweise Texthintergrund müssen auf hellem
und dunklem Canvas gut lesbar bleiben. Die Marker behalten beim Zoomen eine
gut erkennbare Bildschirmgröße; sie skalieren nicht zu winzigen oder
übermäßig großen Weltobjekten.

Schematisch liegen die Beschriftungen so am jeweiligen Kreuz:

```text
Start  |
       |
-------+-------
       |
       |

       |  Ursprung
       |
-------+-------
       |
       |

       |
       |
-------+-------
       |  Kopf
       |
```

Das Ursprungs-Fadenkreuz folgt der gewählten Startreferenz:

- `Aktuelle Position`: Marker an der live gelesenen Kopfposition;
- `Benutzerursprung`: Marker am vom Ruida-Hardwarepanel gelesenen Ursprung;
- gespeicherter Nullpunkt: Marker an dessen gespeichertem X/Y;
- `Absolute Koordinaten`: Marker am Maschinenursprung 0/0.

Ein Wechsel der Auswahl verschiebt den Marker an die Koordinate der neuen
Referenz. Das Ursprungs-Fadenkreuz wird nie allein aufgrund der Auswahl
ausgeblendet. Ist eine live zu lesende Koordinate noch nicht verfügbar, zeigt
das Panel am Marker einen Lade- oder Fehlerzustand, statt ihn an einer
geschätzten oder alten Stelle einzuzeichnen.

Das Start-Fadenkreuz markiert davon getrennt den gewählten 3×3-Jobanker auf
der Job-BBox der aktiven Inhalte (bei „Nur Auswahl“ auf der Auswahl): Es zeigt,
wo **auf den Objekten** der Job beginnt — unabhängig von der gewählten
Startreferenz. An welcher Maschinenkoordinate dieser Anker ausgeführt wird,
zeigt allein das Ursprungs-Fadenkreuz; die beiden fallen nur zusammen, wenn der
Anker tatsächlich auf der Referenzkoordinate liegt. (Revidiert: Eine frühere
Fassung legte **Start** bei relativen Referenzen auf die Referenzkoordinate —
damit klebte er am Ursprung und war informationslos.)

Das Kopf-Fadenkreuz folgt unabhängig davon immer der zuletzt erfolgreich live
gelesenen Kopfposition, solange eine Verbindung besteht. Bei unbekannter oder
veralteter Position wird es nicht an einer geschätzten Stelle gezeichnet.

Wenn zwei oder drei Koordinaten identisch sind, dürfen sich die Marker nicht
gegenseitig unsichtbar machen. Die Darstellung verwendet deshalb
unterschiedliche Kreuzgrößen beziehungsweise Linienstile und feste, versetzte
Beschriftungsquadranten. Alle zutreffenden Farben und Bezeichnungen bleiben
erkennbar; die Zeichenreihenfolge allein darf keine Information verschlucken.

Alle drei Darstellungen verwenden dieselben im Core aufgelösten Koordinaten
und dieselbe `BedOrigin`-Transformation wie die spätere Ausführung. Eine nur
optisch passende Sonderrechnung in Native ist unzulässig.

### C. Werkstück-Nullpunkte gehören zum Laserprofil

Ein benannter Nullpunkt wird als stabile Identität mit Name und absoluten
Maschinenkoordinaten gespeichert:

```rust
pub struct SavedOrigin {
    pub id: String,
    pub name: String,
    pub x_mm: f64,
    pub y_mm: f64,
}

pub struct LaserProfile {
    // bestehende Felder ...
    #[serde(default)]
    pub saved_origins: Vec<SavedOrigin>,
}
```

Die Liste liegt direkt am `LaserProfile`. Damit ist ihre Zuordnung zur stabilen
Laser-ID eindeutig und sie nutzt die bestehende app-globale Persistenz aus ADR
0007. Sie ist projektübergreifend, Bestandteil von Backup/Restore und folgt
einem Profil auch über die bestehende Katalogsynchronisation. Sie wird nicht im
Projektformat gespeichert.

`#[serde(default)]` hält vorhandene `laser-profile.json`-Dateien kompatibel.
Beim Laden werden ungültige Werte und doppelte IDs abgelehnt oder als
beschädigte Profildaten gemeldet; sie werden nicht still umgedeutet.

Ändert sich die Bettgröße oder die konfigurierte Lage des
Maschinen-Nullpunkts eines Profils, validiert Studio alle gespeicherten
Nullpunkte erneut. Ein danach außerhalb des Arbeitsbereichs liegender oder
nicht mehr eindeutig abbildbarer Eintrag bleibt zur Korrektur sichtbar, wird
aber als ungültig markiert. Er kann weder angefahren noch als Jobreferenz
verwendet werden, bis der Nutzer ihn neu speichert oder entfernt.

Der UI-Begriff lautet **„Gespeicherter Nullpunkt“** oder
**„Werkstück-Nullpunkt“**. **„Benutzerursprung“** bleibt ausschließlich die
Bezeichnung für den controllerseitigen Einzelursprung. So sind die beiden
Konzepte trotz ähnlicher Nutzung unterscheidbar.

### D. Speichern der aktuellen Kopfposition

Direkt neben dem „Starten von“-Dropdown bietet das Panel ein Icon
**„Aktuelle Kopfposition als Nullpunkt speichern“** (mit Tooltip) an.
Der Ablauf ist:

1. Ein aktives Laserprofil und eine ausdrückliche Verbindung sind erforderlich.
2. Studio liest beim Auslösen die Kopfposition frisch vom Controller. Ein
   eventuell älterer UI-Anzeigewert wird nicht als Quelle verwendet.
3. Erst nach erfolgreichem Lesen öffnet sich der Namensdialog.
4. Ein leerer oder nur aus Leerzeichen bestehender Name wird abgelehnt.
5. Bestätigen speichert ID, Name und X/Y atomar im aktiven Laserprofil.

Der Nutzer kann einen Eintrag später umbenennen und löschen — das geschieht in
der **Laser-Verwaltung** beim jeweiligen Laserprofil, nicht im Laserpanel.
Umbenennen ändert nicht seine ID. Gleiche Anzeigenamen sollten zur Vermeidung
von Verwechslungen innerhalb eines Profils nicht zugelassen werden.

### E. Auswählen bewegt die Maschine nicht

Die Liste „Starten von“ enthält weiterhin die bestehenden Varianten und ergänzt
die gespeicherten Nullpunkte des aktiven Lasers:

```text
Absolute Koordinaten
Aktuelle Position
Benutzerursprung
Untersetzer Posi
weitere gespeicherte Nullpunkte ...
```

Die bloße Auswahl eines Eintrags löst **keine** Maschinenbewegung aus. Eine
Bewegung erfolgt nur über die gesonderte **„Ursprung“-Kachel** im Job-Grid:
Sie fährt den Bezugspunkt der gewählten Startreferenz laserfrei an (Absolut →
Maschinen-Null 0/0, Benutzerursprung → controllerseitiger Ursprung,
gespeicherter Nullpunkt → dessen Koordinate; bei „Aktuelle Position“ gibt es
nichts anzufahren). Damit kann der Nutzer die Jobreferenz vorbereiten, ohne
dass ein Dropdown unbeabsichtigt den Kopf bewegt.

Das bisherige flache `StartMode` reicht für eine Referenz mit stabiler ID nicht
aus. Das Fachmodell erhält deshalb eine typisierte Auswahl, sinngemäß:

```rust
pub enum StartReference {
    Absolut,
    AktuellePosition,
    Benutzerursprung,
    GespeicherterNullpunkt(String), // SavedOrigin-ID
}
```

Der Anzeigename ist nie die Referenz. Wird ein gespeicherter Nullpunkt gelöscht
oder gehört er nicht zum aktiven Laser, wird Start/Rahmen mit einer klaren
Fehlermeldung abgelehnt. Studio fällt nicht still auf `Absolut`, aktuelle
Position oder einen gleichnamigen Eintrag zurück.

Studio merkt sich die zuletzt verwendete `StartReference` **pro Laserprofil**
und stellt sie nach Programmstart und Profilwechsel wieder her. Die Auswahl ist
eine lokale Bedienpräferenz und wird getrennt von der synchronisierten
Nullpunktliste gespeichert; ein bloßer Auswahlwechsel erzeugt daher keinen
Hub-Katalogkonflikt. Verweist die gemerkte Auswahl auf eine gelöschte oder
ungültige ID, zeigt Studio den fehlenden Bezug sichtbar an und verlangt eine
neue Auswahl, statt still auf `Absolut` zurückzufallen.

### F. Anfahren ist eine geräteneutrale Live-Aktion

`MachineDriver` wird um eine absolute, laserfreie Bewegung ergänzt, sinngemäß:

```rust
fn move_to(
    &self,
    x_mm: f64,
    y_mm: f64,
    speed_mm_s: f64,
) -> Result<(), DriverError>;
```

Der `LaserService` löst die gespeicherte ID ausschließlich im aktiven Profil
auf, prüft endliche Koordinaten sowie die Bettgrenzen und delegiert erst danach
an den Treiber. Das Anfahren ist immer eine Eilbewegung mit ausgeschaltetem
Laser und benötigt eine bestehende Verbindung. Geschwindigkeit stammt aus dem
vorhandenen Jog-/Bewegungswert beziehungsweise einer später ausdrücklich
definierten Profilgrenze.

Während eines laufenden Jobs wird nicht angefahren: Der Application-
Anwendungsfall prüft den Maschinenstatus vor jeder Anfahr-Bewegung, soweit der
Treiber einen Status liefert; eine reine UI-Sperre genügt nicht als
Sicherheitsgrenze.

Ruida führt nach dem Einschalten selbstständig eine Referenzfahrt aus. Studio
fordert deshalb für Ruida kein zusätzliches, derzeit nicht nachgewiesenes
„Homed“-Protokollflag und erzwingt beim Verbinden keine zweite Referenzfahrt.
Vor Speichern oder absolutem Anfahren muss der Treiber jedoch eine aktuelle,
endliche und innerhalb des Profils liegende Maschinenposition erfolgreich
lesen können. Scheitert das Statuslesen, bleibt die Aktion gesperrt. Ein
späterer Treiber, der seinen Referenzzustand explizit melden kann oder vor
absoluten Fahrten ein Homing benötigt, setzt diese zusätzliche Bedingung hinter
dem geräteneutralen Capability-/Statusvertrag um.

### G. Gespeicherter Nullpunkt als Job-Referenz

Ein gespeicherter Nullpunkt bezeichnet den Punkt auf dem Bett, auf den der
gewählte 3×3-Jobanker gelegt wird. Er ist damit eine app-seitig bekannte
absolute Referenz und kein neuer controllerseitiger Startmodus.

Vor Kompilierung, Rahmen und Gummiband löst die Application die Referenz auf und
verschiebt die Jobgeometrie so, dass der gewählte Anker auf den gespeicherten
absoluten X/Y-Koordinaten liegt. Anschließend erhält der Treiber einen absoluten
Job. Dadurch muss kein Treiber einen beliebigen app-seitigen Nullpunkt in sein
Protokoll einbauen, und der Kopf muss vor dem Job nicht zuerst an die Position
gefahren werden.

Die Transformation ist UI-unabhängige, testbare Fachlogik. Sie wird genau
einmal in der gemeinsamen Ausführungsspur angewandt, damit Vorschau, Rahmen,
Gummiband, Export und realer Job nicht auseinanderlaufen (ADR 0015).

`AktuellePosition` und `Benutzerursprung` behalten ihre vorhandene
controllerseitige Semantik. Diese ADR ersetzt sie nicht.

## Architektur und Schichtentrennung

Die Funktion folgt den Architektur-Invarianten aus `CLAUDE.md`, ADR 0001, ADR
0007, ADR 0011 und ADR 0015. Sie rechtfertigt keine Abkürzung zwischen UI und
Treiber und keinen zweiten fachlichen Zustand in Native.

### Core: Fachmodell und reine Berechnung

`studio-core` besitzt die UI- und treiberunabhängigen Fachtypen und Regeln:

- `SavedOrigin` und die Zuordnung zum `LaserProfile`;
- stabile IDs, Namen, Koordinaten und Validierungsregeln;
- `StartReference` und die fachliche Bedeutung jeder Variante;
- Auflösung von Jobanker und Referenzkoordinate;
- reine Transformationen zwischen Bett-, Maschinen- und Jobgeometrie;
- darstellungsneutrale Markerinformationen wie Art und Weltkoordinate, soweit
  sie aus Fachzustand berechnet werden müssen.

Der Core kennt weder egui noch Farben, Dialoge, Polling, UDP-Pakete oder
Ruida-Register. Seine Berechnungen sind ohne UI und ohne Hardware testbar.

### Treiber: Hardware und Protokoll

Ein Maschinentreiber bleibt ausschließlich für das konkrete Gerät und dessen
Protokoll verantwortlich:

- Verbindung und Transport;
- Lesen der realen Kopfposition;
- Lesen des controllerseitigen Benutzerursprungs;
- sicheres Umsetzen eines absoluten Fahrbefehls;
- gerätespezifisches Kompilieren und Ausführen der bereits aufgelösten
  Jobparameter.

Der Treiber kennt keine Dialoge, Farben, Canvas-Marker oder Anzeigenamen
gespeicherter Nullpunkte. Er speichert keine app-seitige Nullpunktliste und
entscheidet nicht, welcher Eintrag im Laserpanel ausgewählt ist. Ruida-
Registeradressen und Paketaufbau verlassen das Ruida-Treiber-Crate nicht.

### Application: Anwendungsfälle und Orchestrierung

`studio-application` bildet die einzige Koordinationsgrenze zwischen Native,
Core, Persistenz und Treiber. Der `LaserService`:

- prüft aktives Profil und Verbindungszustand;
- liest Status und Benutzerursprung über `MachineDriver`;
- löst eine gespeicherte ID nur innerhalb des aktiven Laserprofils auf;
- validiert Bewegungsziel, Bettgrenzen und Maschinenzustand;
- koordiniert Speichern, Umbenennen, Löschen und persistentes Schreiben;
- liefert Native einen fertigen, geräteneutralen Anzeigestand;
- delegiert Bewegung und Jobausführung an den aktiven Treiber.

Application erzeugt keine egui-Widgets und zeichnet keine Fadenkreuze. Sie
enthält keine Ruida-Paketbytes und dupliziert keine Geometrierechnung aus dem
Core.

### Native: Anzeige und Benutzerabsichten

`studio/native` bleibt Präsentationsschicht:

- zeigt die von Application gelieferten Positionen und Zustände;
- zeichnet `Start`, `Ursprung` und `Kopf` mit den festgelegten Farben,
  Beschriftungen und Linienstilen;
- öffnet Namens- und Bestätigungsdialoge;
- übersetzt Klicks und Auswahlwechsel in typisierte `UiAction`s;
- hält ausschließlich kurzlebigen Dialog-, Lade- und Darstellungszustand.

Native liest und schreibt `laser-profile.json` nicht selbst, greift nicht auf
Ruida-Register zu und sendet keine Bewegungspakete. Eine im UI deaktivierte
Schaltfläche ist nur Bedienhilfe; sicherheitsrelevante Prüfungen bleiben
zusätzlich im Application-Anwendungsfall.

### Hub: kompatible Sicherung und Synchronisation

Gespeicherte Nullpunkte sind Bestandteil des bestehenden
`CatalogKind::LaserProfile` und werden nicht als paralleler Hub-Datentyp
eingeführt. Jede lokale Mutation der Nullpunktliste erzeugt über die vorhandene
Application-Outbox einen neuen Katalogeintrag für die stabile Laserprofil-ID:

- Nullpunkt anlegen;
- umbenennen;
- Koordinate aktualisieren, falls später angeboten;
- Nullpunkt löschen;
- Laserprofil mitsamt Nullpunkten löschen.

Ein vom Hub empfangenes Laserprofil wird ausschließlich über den bestehenden
`LaserService::apply_shared_record`-Anwendungsfall validiert, lokal gespeichert
und in den laufenden Zustand übernommen. Native schreibt empfangene Payloads
nicht selbst. Offline vorgenommene Änderungen bleiben in der dauerhaften Outbox
und werden beim nächsten erfolgreichen Sync übertragen.

Die JSON-Erweiterung bleibt in beide Richtungen kontrolliert kompatibel:

- ein altes Profil ohne `saved_origins` wird als leere Liste geladen;
- ein aktueller Hub validiert IDs, Namen, endliche Koordinaten, Bettgrenzen und
  die Zuordnung zum Profil;
- der Hub speichert und verteilt den vollständigen Profil-Payload einschließlich
  `saved_origins` und verändert Koordinaten nicht;
- Backup und Restore enthalten dieselbe vollständige Liste;
- Inhalts-Hash und Konflikterkennung beziehen die Nullpunktliste ein.

Da der gemeinsame Katalog heute auf Ebene des gesamten Laserprofils
versioniert wird, werden konkurrierende Profiländerungen nicht feldweise oder
listenweise still zusammengeführt. Ein Konflikt bleibt sichtbar und muss über
den bestehenden Konfliktablauf entschieden werden. Insbesondere darf ein
älterer Client eine bereits synchronisierte Nullpunktliste nicht unbemerkt
durch erneutes Speichern eines ihm unbekannten Profils entfernen.

Der Laserprofil-Payload erhält deshalb verbindlich eine ganzzahlige
`schema_version`. Bestehende Profile ohne Feld gelten als Version 1; die erste
Version mit `saved_origins` erhält Version 2. Studio schreibt immer die höchste
von ihm vollständig verstandene Version. Der Hub merkt sich die Version des
aktuellen Profildatensatzes und lehnt ein Zurückschreiben mit kleinerer Version
als Schema-Downgrade ab. Ein Client ohne Unterstützung für `saved_origins` darf
ein entsprechend neueres Profil lesen beziehungsweise anzeigen, aber nicht
verlustbehaftet zurückschreiben. Studio und Hub erhalten dafür gemeinsame
Roundtrip- und Mischversions-Tests.

Der kompatible Rollout aktualisiert deshalb zuerst den Hub um die
Schema-Downgrade-Prüfung und danach Studio. Studio prüft die vom Hub gemeldete
Protokoll-/Capability-Version, bevor es ein Laserprofil der Version 2
synchronisiert. Ein alter Hub ohne diesen Schutz blockiert nur den
Profilkatalog-Sync mit einer verständlichen Meldung; die lokale Arbeit und
andere kompatible Hub-Funktionen bleiben davon unberührt.

Der Hub bleibt optional. Lokales Speichern, Anzeigen und Anfahren funktionieren
ohne Hub-Verbindung; Synchronisation ergänzt diese Arbeitsweise, ersetzt die
lokale Registry aber nicht.

Der Datenfluss bleibt damit eindeutig:

```text
Benutzereingabe
    ↓
Native: UiAction / Darstellung
    ↓
Application: Anwendungsfall, Validierung, Persistenzkoordination
    ↓                         ↓
Core: Fachmodell/Berechnung     MachineDriver: Hardware/Protokoll
    ↑                         ↑
    └─────── fertiger Anzeigestand ───────┘
                    ↓
              Native: Canvas/UI
```

Kein Pfeil führt direkt von Native zum konkreten Ruida-Treiber. Kein Treiber
greift auf Core-Persistenz oder UI-Zustand zu.

## Invarianten

1. Kopfposition und Benutzerursprung werden vom aktiven Treiber gelesen; die UI
   berechnet oder erfindet sie nicht.
2. Die Koordinate des Hardware-Benutzerursprungs wird nur bei gewähltem
   `Benutzerursprung` verwendet und bezeichnet ausschließlich den am
   Ruida-Hardwarepanel gesetzten Nullpunkt; das allgemeine Fadenkreuz
   `Ursprung` bleibt unabhängig davon immer sichtbar.
3. Der Canvas unterscheidet die Fadenkreuze `Start`, `Ursprung` und `Kopf`
   durch feste Farben und sichtbare Textbeschriftungen; deckungsgleiche Marker
   bleiben unterscheidbar.
4. `Ursprung` ist bei jeder Auswahl sichtbar und wandert an deren tatsächliche
   Referenzkoordinate; bei `Absolut` steht er am Maschinenursprung 0/0.
5. Ein gespeicherter Nullpunkt gehört genau zu einem Laserprofil und kann nie
   still mit einem anderen Laser verwendet werden.
6. Die aktuelle Position wird beim Speichern frisch gelesen; ein gecachter
   Anzeigewert ist nicht autoritativ.
7. Auswahl und Maschinenbewegung sind getrennt. Nur eine ausdrückliche Aktion
   fährt den Kopf an.
8. Jede absolute Bewegung wird vor dem Treiberaufruf gegen endliche Werte und
   die Bettgrenzen des aktiven Profils geprüft.
9. Anfahren erfolgt ohne Laserleistung und nicht während eines laufenden Jobs.
10. Gespeicherte Referenzen verwenden stabile IDs, niemals Namen oder
   Listenindizes.
11. Eine fehlende Referenz erzeugt einen Fehler; es gibt keinen stillen
   Fallback.
12. Canvas, Vorschau, Rahmen, Export und Job verwenden dieselbe aufgelöste
    Referenz und Geometrie.
13. Die Funktion bleibt offline-first und projektunabhängig; der Hub ist weder
    für Speichern noch Anfahren erforderlich.
14. Native kommuniziert niemals direkt mit einem konkreten Treiber; jeder
    Maschinenzugriff läuft über einen Application-Anwendungsfall und den
    geräteneutralen `MachineDriver`-Vertrag.
15. Fachmodell und Koordinatenrechnung liegen im Core, Orchestrierung und
    Persistenz in Application, Hardwareprotokoll im Treiber und ausschließlich
    Darstellung sowie kurzlebiger UI-Zustand in Native.
16. Jede Änderung an gespeicherten Nullpunkten wird als vollständiges
    `LaserProfile` über den bestehenden gemeinsamen Katalog synchronisiert und
    ist Bestandteil von Hub-Backup/Restore.
17. Versionsunterschiede dürfen `saved_origins` nicht still entfernen; ein
    nicht verlustfreier Schema-Downgrade wird abgelehnt oder als sichtbarer
    Konflikt behandelt.
18. Die zuletzt verwendete Startreferenz wird pro Laser lokal wiederhergestellt
    und nicht als häufig wechselnde Bedienpräferenz mit dem Profilkatalog
    synchronisiert.
19. Ruida benötigt kein erfundenes Homing-Flag; ohne erfolgreich gelesene,
    plausible Maschinenposition sind Speichern und absolutes Anfahren dennoch
    gesperrt.

## Konsequenzen

- Die in der Native-Version fehlenden Positionsinformationen stehen wieder im
  Arbeitsablauf zur Verfügung.
- Der Canvas macht Startposition, gewählten Bezugspunkt und reale Kopfposition
  gleichzeitig sichtbar. Der Ursprungsmarker bleibt bei jeder Startart als
  stabiler visueller Bezug erhalten.
- Feste Halterungen und wiederkehrende Werkstückpositionen lassen sich ohne
  erneutes manuelles Ausrichten verwenden.
- Mehrere benannte Positionen ergänzen den einzelnen controllerseitigen
  Benutzerursprung, ohne dessen Bedeutung zu verändern.
- `LaserProfile` und dessen JSON-/Sync-Repräsentation wachsen um eine Liste.
- `StartMode` beziehungsweise seine Aufrufer müssen zu einer referenzfähigen
  Modellierung migriert werden.
- Die gemeinsame Jobtransformation wird um das Platzieren eines Ankers auf eine
  absolute Referenz erweitert.
- Treiber, die kein Statuslesen oder absolutes Anfahren unterstützen, können
  die Anzeige beziehungsweise Aktion gezielt als nicht unterstützt melden.

## Reihenfolge der Umsetzung

1. Application-Anwendungsfälle für `status()` und das nur bei Bedarf genutzte
   `read_origin()` bereitstellen und Fehler-/Capability-Verhalten testen.
2. Kopfposition und die von „Starten von“ abhängige Ursprungsanzeige im nativen
   Laserpanel wiederherstellen; Aktualisierung nach Verbindung und Bewegungen
   ergänzen.
3. Gemeinsame Auflösung der Startreferenz im Core/Application-Layer schaffen
   und die Fadenkreuze `Start`, `Ursprung` und `Kopf` mit Farbe,
   Beschriftungsquadrant und definiertem Überlagerungsverhalten darstellen;
   bei `Absolut` den Ursprung an 0/0 setzen.
4. `schema_version`, `SavedOrigin` und `LaserProfile.saved_origins` mit
   Validierung, kompatibler Deserialisierung und Core-Tests einführen;
   Revalidierung nach Bett-/Nullpunktänderungen abdecken.
5. Speichern, Umbenennen und Löschen im `LaserService` implementieren;
   persistentes Schreiben und Katalogsynchronisation testen.
6. Namensdialog und Nullpunktliste im Laserpanel ergänzen.
7. `MachineDriver::move_to` sowie Ruida-Implementierung, Grenzprüfung und
   Application-Aktion für „Anfahren“ bauen.
8. `StartReference` einführen und gespeicherte Referenzen in der gemeinsamen
   Ausführungsspur für Vorschau, Rahmen, Export und Start auflösen.
9. Fehlerfälle testen: getrenntes Gerät, fehlender Nullpunkt, Profilwechsel,
   außerhalb des Betts, geänderte Profilgeometrie, nicht lesbare Kopfposition,
   laufender Job und nicht unterstützter Treiber; letzte Auswahl pro Laser
   wiederherstellen.
10. Hub-Kompatibilität testen: altes Profil ohne Liste, vollständiger
    Studio→Hub→Studio-Roundtrip, Offline-Outbox, Backup/Restore, Konflikt und
    Schutz vor verlustbehaftetem Schreiben durch einen älteren Client; Hub
    zuerst und Studio danach ausrollen.
11. Ruida-Hardwaretest: die vier Auswahlarten nacheinander prüfen, Marker und
    reale Koordinate vergleichen, Position speichern, wiederholt anfahren und
    denselben Testjob mit unterschiedlichen Ankern reproduzierbar platzieren.

## Nicht Teil dieser Entscheidung

- Schreiben oder Ersetzen des controllerseitigen Benutzerursprungs.
- Automatisches Anfahren allein durch Auswahl im Dropdown.
- Projektbezogene Nullpunkte oder das Speichern einer Position im
  Projektformat.
- Maschinenübergreifendes Kopieren von Nullpunkten.
- Kamera-, Vision- oder Fiducial-Ausrichtung.
- Automatische Korrektur mechanischer Abweichungen einer Halterung.
