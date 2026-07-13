# ADR 0012: Charon als optionaler lokaler Koordinationsdienst

## Status

Akzeptiert — 2026-07-13 · Lokaler Funktionsumfang einschließlich
Ruida-Lease-Koordination umgesetzt.

## Kontext

Charon soll Projektstände zwischen Office und Workshop verteilen, die
regelmäßige Proxmox-Sicherung als zentralen Ablagepunkt nutzen,
arbeitsplatzbezogene Einstellungen sichern und konkurrierende Zugriffe auf
einen Ethernet-Ruida koordinieren. Gleichzeitig gilt unverändert:
**LuxiFer first, Charon optional.** Editor, lokales Speichern und Laserbetrieb
müssen ohne Charon möglich bleiben.

Der erste Entwicklungsschritt soll auf demselben Rechner wie LuxiFer laufen.
Damit können Protokoll, Fehlergrenzen und Bedienung stabilisiert werden, bevor
Deployment, Authentifizierung oder ein Proxmox-Betrieb hinzukommen.

## Entscheidung

Charon beginnt als **optional aktivierter lokaler HTTP-Dienst**. Der aktuelle
Ausbaustand umfasst Erreichbarkeit, Protokollaushandlung, Mehrinstanz-Präsenz
sowie Projekt- und Asset-Synchronisierung:

- Standardbindung: `127.0.0.1:3737`; keine Freigabe ins LAN;
- `GET /health` bestätigt nur die Prozessbereitschaft;
- `GET /api/v1/handshake` liefert JSON mit Serverversion, Protokollversion,
  Instanzkennung und expliziten Fähigkeiten;
- `POST /api/v1/workplaces/heartbeat` registriert die stabile Arbeitsplatz-ID
  mit ihrem sichtbaren Namen; `GET /api/v1/workplaces` liefert den aktuellen
  Anwesenheits-Snapshot;
- die native Anwendung erhält eine globale Charon-Einstellung mit Aktivierung,
  Basis-URL, Verbindungstest und verständlichem Status;
- die Application-Schicht besitzt Netzwerkzugriff und Fehlerübersetzung; egui
  stellt nur Draft und Ergebnis dar;
- lokale Outbox und Inbox sichern den Projekttransfer gegen Prozess- und
  Netzwerkausfälle; ein cursorbasierter Long-Poll verkürzt die Zustellung;
- bestehende Projekte werden read-only verglichen und anschließend bewusst als
  lokale oder empfangene Gesamtversion aufgelöst;
- ein nicht gestarteter oder nicht erreichbarer Charon beeinträchtigt weder
  Editor, Projekte noch Laserbetrieb.

Die erste Protokollversion ist `1`. Fähigkeiten werden als stabile String-IDs
gemeldet. Der Server meldet `health`, `handshake`, `workplaces`,
  `project_revisions`, `project_events`, `assets`, `workplace_backups` und
  `machine_leases`;
  unbekannte Fähigkeiten müssen von Clients ignoriert werden.

## Invarianten

1. Charon steuert niemals direkt eine Maschine und besitzt keinen
   `MachineDriver`.
2. Charon ist kein Speicher-Wahrheitszentrum für den Editor. Lokale Dateien und
   der Core bleiben ohne Server vollständig nutzbar.
3. Netzwerk- und JSON-Details gelangen nicht in egui-Callbacks und nicht in
   `luxifer-core`.
4. Eine Bindung außerhalb des Loopback-Interfaces ist später eine bewusste
   Betriebsentscheidung mit eigener Authentifizierungs- und TLS-Grenze.
5. Handshake-Kompatibilität wird über die Protokollversion entschieden, nicht
   über die Charon-Binaryversion.
6. **Lokales Speichern kommt zuerst.** Ein Speichervorgang schreibt immer zuerst
   die lokale Projektversion und endet unabhängig vom Charon-Ergebnis
   erfolgreich. Eine persistente Outbox überträgt neue Versionen später und
   wiederholt fehlgeschlagene Übertragungen nach Neustarts.
7. **Charon verteilt Versionen, verändert sie aber nicht.** Charon speichert
   empfangene Projektversionen inhaltsgetreu, katalogisiert Elternbeziehung,
   Arbeitsplatz und Hash und meldet sie anderen verbundenen Arbeitsplätzen per
   Push. Er editiert oder merged keine Projektinhalte und überschreibt keine
   lokale Datei selbst.
8. **Der empfangende Client entscheidet.** Neue Versionen landen zunächst in
   einer lokalen Inbox. Bei geöffneten, ungespeicherten oder abweichenden
   Projekten zeigt LuxiFer `Übernehmen`, `Später`, `Änderungen anzeigen` und
   später einen expliziten Merge-Ablauf. Konflikte bleiben als parallele
   Versionszweige erhalten; Charon bestimmt keinen Gewinner.
9. **Arbeitsplätze haben stabile Identität.** Ein unsichtbarer stabiler
   `workplace_id` identifiziert den Rechner; `workplace_name` ist der
   menschenlesbare Name. UI-Settings und Laserprofile werden je Arbeitsplatz
   als versionierte Sicherungen abgelegt. Eine Neuinstallation lädt und
   übernimmt sie nur nach ausdrücklicher Auswahl durch den Nutzer.
10. **Ruida-Exklusivität ist Koordination, keine Maschinensteuerung.** Charon
   vergibt genau eine zeitlich begrenzte Lease pro Ethernet-Controller.
   Nur der Lease-Inhaber verbindet sich selbst mit dem Ruida; Charon sendet
   niemals Maschinenbefehle oder Jobdaten.
11. **Die Verbindung bleibt manuell.** `Verbinden` im Laser-Tab fordert die
   Lease an. Hält ein anderer Arbeitsplatz eine untätige Verbindung, fordert
   Charon ihn per Push zum Trennen auf und übergibt anschließend die Lease.
   Läuft oder pausiert ein Job, wird die Übergabe abgelehnt. Kurze kritische
   Controller-Schreibvorgänge gelten ebenfalls als belegt.
12. **Verwaiste Leases sperren nicht dauerhaft.** Heartbeats halten eine Lease
   aktiv. Nach Ablauf darf eine zuletzt sicher untätige Lease automatisch
   freigegeben werden. Bei `Running`, `Paused` oder unbekanntem letzten Status
   ist nur eine deutlich bestätigte Zwangsfreigabe nach Kontrolle an der
   Maschine zulässig. Ein Prozentfortschritt ist für die Lease nicht nötig.
13. **Charon-Ausfall bleibt beherrschbar.** Ist Charon nicht konfiguriert, darf
   LuxiFer direkt verbinden. Ist er konfiguriert, aber nicht erreichbar, warnt
   LuxiFer vor einer unkoordinierten Ethernet-Verbindung und verlangt eine
    manuelle Bestätigung. USB-Verbindungen benötigen keine Charon-Lease.
14. **Die Asset-Bibliothek ist lokal vollständig.** Name, automatisch
    abgeleitete Tags, Suche, Thumbnails und Wiederverwendung funktionieren ohne
    Charon. Charon synchronisiert nur Asset-Bytes und zusammenführbare
    Metadaten; es erzeugt weder die lokale Bibliothek noch ihre Vorschaubilder.

## Nicht Teil von v1.0

- Assetübertragung und Deduplizierung (direkt nach `v1.0` ergänzt);
- Drei-Wege-Merge einzelner Shapes oder Layer;
- Aufräum- und Aufbewahrungsregeln für bestätigte Sync-Revisionen;
- Benutzerkonten, Tokens, TLS, Discovery oder Fernzugriff;
- Maschinen-Queueing oder Jobübertragung durch Charon;
- Proxmox-, Container- oder Systemdienst-Deployment.

## Nächste Schritte

1. Charon auf Proxmox als gesicherten Systemdienst bereitstellen; Freigabe ins
   LAN erst zusammen mit Authentifizierung und TLS.
2. Empfangsbestätigungen für definierte Aufräum- und Aufbewahrungsregeln nutzen.
3. Optional stabile Shape-/Layer-IDs und einen Drei-Wege-Objekt-Merge
   vorbereiten; bis dahin bleiben Konfliktentscheidungen auf Versionsebene.

## Umsetzungsstand

Der erste Meilenstein ist mit Tag `v1.0` umgesetzt:

- Charon bindet standardmäßig und erzwungen an `127.0.0.1:3737`;
- Health und Handshake antworten mit JSON und wurden gegen einen real
  gestarteten lokalen Prozess geprüft;
- der Client liegt in `luxifer-application`, validiert URL, HTTP-Status,
  Serverkennung und Protokollversion und übersetzt Fehler in `AppError`;
- Aktivierung, URL und Verbindungstest liegen in der globalen
  Charon-Einstellungssektion; alte Settings erhalten sichere Defaults;
- jeder Datenbereich erhält beim ersten Start eine persistierte
  `workplace_id`; der sichtbare Arbeitsplatzname bleibt frei änderbar;
- der Verbindungstest registriert den Arbeitsplatz und zeigt Charons bekannten
  Anwesenheits-Snapshot;
- bei aktivierter Charon-Koordination meldet sich LuxiFer regelmäßig aus einem
  Hintergrundthread. Netzwerkfehler blockieren den UI-Thread nicht
  und werden als getrennter Zustand sichtbar; nach 15 Sekunden ohne Meldung
  gilt ein Arbeitsplatz als offline;
- Charon bietet zusätzlich `GET /api/v1/events/projects` als cursorbasierten
  Long-Poll-Kanal an. Der Server bearbeitet Verbindungen parallel und weckt
  wartende Clients unmittelbar nach einem erfolgreichen Revisionsupload;
  nach vier Sekunden endet ein unverändertes Warten regulär. LuxiFer führt den
  Kanal ausschließlich im bestehenden Hintergrundthread und niemals im
  UI-Thread aus;
- der Long-Poll ersetzt nicht Heartbeat, Outbox oder Inbox: Er verkürzt nur die
  Zeit bis zum nächsten bereits idempotenten Sync. Nach Verbindungsabbruch oder
  Charon-Neustart erkennt LuxiFer die neue Server-Instanz-ID und startet den
  Cursor gefahrlos erneut bei null;
- Charon hält die Registrierung vorerst nur im Arbeitsspeicher. Ein Neustart
  leert die Anwesenheitsliste, die laufenden Clients melden sich selbstständig
  wieder an;
- `scripts/run-local-charon-demo.sh` startet Charon, Office und Workshop mit
  voneinander isolierten Datenverzeichnissen in drei Terminals;
- nach jedem erfolgreichen lokalen Speichern legt LuxiFer bei aktiviertem
  Charon einen atomar geschriebenen Outbox-Eintrag unter
  `sync/outbox/<revision_id>/` an. Manifest und eigene `payload.luxi`-Kopie
  bleiben auch bei einem späteren Strg+S unverändert;
- Sync-Revisionen sind von den sichtbaren Projektversionen getrennt. Sie tragen
  Projekt-/Versions-/Arbeitsplatz-ID, Elternrevision, Zeitpunkt, Inhaltshash
  und Status. Dadurch bildet auch mehrfaches Speichern innerhalb etwa V1 eine
  eindeutige, konfliktfähige Kette;
- ein Outbox-Fehler macht das zuvor erfolgreiche lokale Speichern nicht
  rückgängig und wird als separate Warnung angezeigt;
- der Hintergrunddienst überträgt offene und fehlgeschlagene Outbox-Einträge
  nach einem erfolgreichen Heartbeat in Reihenfolge ihrer Revisionskette;
- Charon prüft den Inhaltshash und speichert Manifest und Payload atomar unter
  `projects/<project_id>/revisions/<revision_id>/`. Die Ablagewurzel ist über
  `CHARON_DATA_DIR` konfigurierbar;
- erst eine passende Bestätigung aus Revisions-ID und Hash setzt den lokalen
  Eintrag auf `uploaded`. Fehlgeschlagene Übertragungen bleiben mit Fehlertext
  erhalten und werden beim nächsten Heartbeat erneut versucht;
- wiederholte identische Uploads sind idempotent. Dieselbe Revisions-ID mit
  anderem Inhalt wird als Konflikt abgelehnt;
- der lokale HTTP-Server liest vollständige Requests bis 64 MiB statt nur den
  ersten Netzwerkblock;
- Charon liefert einem Arbeitsplatz ausschließlich Revisionen anderer
  Arbeitsplätze. LuxiFer prüft deren Hash und legt sie idempotent und atomar
  unter `sync/inbox/<revision_id>/` ab;
- Inbox-Einträge starten mit `pending_review`. Empfangene Payloads verändern
  weder den Canvas noch lokale Projektdateien automatisch;
- neue Inbox-Einträge werden per Toast gemeldet. Bis zur serverseitigen
  Empfangsbestätigung verhindert die lokale idempotente Ablage Duplikate;
- erst nach erfolgreicher, hashgeprüfter Inbox-Ablage bestätigt LuxiFer die
  Revision bei Charon. Charon speichert den Beleg pro Arbeitsplatz atomar unter
  `receipts/<workplace_id>/<revision_id>.json` und liefert bestätigte Stände an
  diesen Arbeitsplatz nicht erneut aus;
- geht die Bestätigung unterwegs verloren, wird die Revision noch einmal
  geliefert, lokal als bereits vorhanden erkannt und erneut bestätigt. Damit
  bleibt der Ablauf auch über Prozess- und Netzwerkausfälle hinweg sicher;
- der Projekt-Reiter besitzt den Bereich `Von Charon`; ein Badge am
  Projekt-Einstieg zählt neue `pending_review`-Revisionen. `Später` setzt sie
  auf `deferred`, ohne sie zu löschen;
- `Übernehmen` unterscheidet anhand der stabilen Projekt-ID: Ein unbekanntes
  Projekt wird lokal angelegt, eine weitere Revision eines vorhandenen Projekts
  wird als neue lokale Version übernommen. Ist genau dieses Projekt geöffnet
  und ungespeichert, greift vorher der Dirty-Guard;
- ein globaler, content-adressierter Asset-Katalog hält normalisierte Bilder,
  verwendete Fonts und unveränderte SVG-/DXF-Quelldateien. Gleiche Bytes werden
  anhand ihres Hashs nur einmal abgelegt;
- Asset-Metadaten tragen Such-Tags aus dem ursprünglichen Dateinamen sowie aus
  Namen und Beschreibung der Projekte, in denen das Asset importiert oder als
  Bildreferenz gespeichert wurde. Neue Zusammenhänge ergänzen die Tags
  idempotent, ohne Content-ID oder Asset-Bytes zu verändern. Beim Start werden
  vorhandene Projektreferenzen einmal gegen den Katalog zurückgespielt, sodass
  auch ältere Bild-/Font-Assets nach ihrem bisherigen Projekt auffindbar sind;
- der lokale Asset-Katalog zeigt gecachte Thumbnails für Bilder, SVG und DXF.
  Die Suche filtert unmittelbar über Dateiname und Tags; ein Doppelklick oder
  `Einfügen` verwendet das Asset über die bestehende Import-Pipeline erneut;
- Thumbnail-PNGs sind jederzeit neu erzeugbare lokale Cache-Dateien. Sie werden
  nicht über Charon übertragen. Tag-Metadaten werden beim Asset-Sync als
  Mengenvereinigung zusammengeführt, damit Projektkontext verschiedener
  Arbeitsplätze erhalten bleibt;
- der Programmstart lädt ausschließlich den Asset-Metadatenkatalog. Die
  virtualisierte Trefferliste fordert Thumbnails nur für gerade sichtbare
  Karten an; ein deduplizierender Hintergrundthread liest oder erzeugt sie und
  der UI-Thread übernimmt lediglich fertige Pixel als egui-Texturen. Dadurch
  blockieren auch mehrere tausend neue Assets weder Start noch Suche;
- Charon bietet den Katalog über `GET /api/v1/assets`,
  `GET /api/v1/assets/<id>` und `POST /api/v1/assets` an. Der Hintergrunddienst
  gleicht Assets vor den Projektrevisionen bidirektional ab und überprüft beim
  Empfang erneut den Inhaltshash;
- Projektdateien referenzieren Bild- und Font-Assets über stabile IDs. Beim
  Empfang wird die lokale Verfügbarkeit aller Abhängigkeiten geprüft und der
  Fontpfad auf die lokale Katalogdatei aufgelöst;
- originale SVG- und DXF-Uploads bleiben zusätzlich zur daraus erzeugten
  Geometrie erhalten. Im Projekt-Reiter listet `Assets` Bilder sowie diese
  Vektorquellen und fügt sie über die reguläre Import-Pipeline in neue Projekte
  ein. Katalogisierte Fonts erscheinen stattdessen in der Schrift-Auswahl;
- Projekt- und Asset-Katalog werden beim Start einmal geladen und im nativen
  App-Zustand gehalten. Sie werden nur nach tatsächlichen Projektänderungen,
  Importen oder Charon-Downloads aktualisiert; der UI-Framepfad liest und parst
  keine Katalogdateien vom Datenträger;
- nach erfolgreichem Import erscheint das Projekt in `Meine Projekte`; der
  Canvas und ein eventuell geöffnetes, ungespeichertes Projekt werden nicht
  automatisch ersetzt;
- `Änderungen anzeigen` öffnet einen strikt read-only Vergleich. Lokales Projekt
  und Charon-Revision werden über die stabile Projekt-ID zugeordnet und mit
  getrennten Miniaturen, Größen sowie Änderungsmarkern für Arbeitsbereich,
  Ebenen, Objekte und Metadaten angezeigt. Der Dialog verändert weder Inbox-
  Status noch Projektdateien;
- `Lokale Version behalten` quittiert ausschließlich die konkrete Inbox-
  Revision als ignoriert. `Charon-Version übernehmen` hängt den empfangenen
  Stand als neue lokale Version mit eigener Versions-ID und Herkunftsnotiz an;
  die lokale Historie bleibt erhalten. Ist genau dieses Projekt geöffnet und
  ungespeichert, greift vorher der Dirty-Guard. Fehlt eine referenzierte
  Asset-Datei trotz Synchronisierung, bleibt die Übernahme sicher gesperrt.
- Charon hält je Arbeitsplatz den jüngsten Snapshot von `ui_settings` und
  `laser_profiles` getrennt und atomar unter `workplaces/<workplace_id>/`.
  Inhaltshashes sichern die Übertragung ab; identische Snapshots werden
  idempotent bestätigt und nicht neu geschrieben;
- LuxiFer übergibt lokale Änderungen an Settings und Laserprofilen nur dem
  vorhandenen Hintergrundthread. Netzwerkzugriff findet weder beim Speichern
  noch im egui-Callback statt und ein Charon-Fehler macht die lokale Änderung
  nicht rückgängig;
- der Charon-Dialog lädt vorhandene Arbeitsplatzsicherungen ausdrücklich auf
  Nutzerwunsch. Settings und Laserprofile werden getrennt aufgeführt und erst
  durch `Wiederherstellen` lokal geschrieben; beim Start erfolgt keine
  automatische Übernahme.
- der Laser-Tab zeigt für das aktive Profil ausdrücklich `Verbinden` oder
  `Trennen` samt sichtbarem Zustand. Erst `Verbinden` baut den Treibertransport
  zum konfigurierten Ziel auf; Profilwechsel, Profiländerung, Löschen und
  Wiederherstellen verwerfen eine bestehende Verbindung;
- Job-Aktionen, Jog und Home verbinden nicht mehr implizit. Im getrennten
  Zustand bleiben ihre Bedienelemente deaktiviert und die Application-Schicht
  weist dennoch jeden maschinenwirksamen Direktaufruf mit
  `laser_not_connected` ab. Reiner Dateiexport bleibt ohne Verbindung möglich.
- Ist Charon für ein Ethernet-Profil aktiviert, aber nicht erreichbar, verlangt
  `Verbinden` vor dem direkten Zugriff eine deutliche Bestätigung. Serielle
  Profile benötigen diese Koordinationswarnung nicht;
- Ethernet-Profile leiten aus der Zieladresse eine arbeitsplatzunabhängige
  Controller-ID ab. `Verbinden` fordert über
  `POST /api/v1/leases/acquire` eine exklusive Lease an; erst nach ihrer
  Bestätigung verbindet LuxiFer selbst den Maschinentreiber;
- Charon hält Leases ausschließlich als Koordinationszustand im Speicher. Ihre
  Laufzeit beträgt 15 Sekunden, LuxiFer erneuert sie alle fünf Sekunden über
  `POST /api/v1/leases/heartbeat`; Charon besitzt weiterhin keinen Treiber und
  überträgt weder Maschinenbefehle noch Jobdaten;
- fordert ein zweiter Arbeitsplatz einen untätig belegten Controller an,
  meldet der nächste Heartbeat dies dem Halter. Dieser trennt und gibt die
  Lease frei; der wartende Client versucht die Übernahme selbstständig erneut;
- `Running`, `Paused` und `Unknown` verhindern eine reguläre Übergabe. Senden
  und Pausieren aktualisieren den Lease-Zustand konservativ; kritische
  Controller-Lese- und Schreibvorgänge melden vorübergehend `Unknown`;
- eine abgelaufene, zuletzt untätige Lease darf Charon automatisch neu
  vergeben. Bei einem abgelaufenen nicht sicheren Zustand verlangt LuxiFer vor
  der Zwangsübernahme eine auffällige Bestätigung, dass Maschine und Auftrag
  vor Ort kontrolliert wurden;
- Trennen, Profilwechsel und Konfigurationswechsel geben eine aktive Lease
  bestmöglich frei. Geht Heartbeat oder Lease verloren, trennt LuxiFer den
  eigenen Treiber und meldet den Fehler sichtbar.

Damit ist der lokale Funktionsumfang dieses ADR abgeschlossen. Noch offen sind
die ausdrücklich nachgelagerten Betriebs- und Ausbaupunkte:
Proxmox-/LAN-Betrieb mit Authentifizierung und TLS, Aufbewahrungsregeln und
optional ein späterer Objekt-Merge. Charon darf Versionen verteilen und
Verbindungen koordinieren, aber keine Projektinhalte selbst bearbeiten,
Maschinen steuern oder laufende Jobs unterbrechen.
