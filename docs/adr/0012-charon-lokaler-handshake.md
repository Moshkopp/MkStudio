# ADR 0012: Charon als optionaler lokaler Koordinationsdienst

## Status

Akzeptiert — 2026-07-13 · präzisiert nach Rollen-/Lease-Entscheidung.

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

Charon beginnt als **optional aktivierter lokaler HTTP-Dienst**. Der erste
Meilenstein enthält Erreichbarkeit, Protokollaushandlung und die kleinste
Mehrinstanz-Basis:

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
- ein nicht gestarteter oder nicht erreichbarer Charon beeinträchtigt weder
  Editor, Projekte noch Laserbetrieb.

Die erste Protokollversion ist `1`. Fähigkeiten werden als stabile String-IDs
gemeldet. Der erste Server meldet `health`, `handshake` und `workplaces`;
unbekannte
Fähigkeiten müssen von Clients ignoriert werden.

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
   vergibt später genau eine zeitlich begrenzte Lease pro Ethernet-Controller.
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

## Nicht Teil dieses Meilensteins

- Übernahme aus der Projekt-Inbox, Empfangsbestätigung und Push-Kanal;
- Settings-/Laserprofil-Sicherung;
- Assetübertragung und Deduplizierung;
- Benutzerkonten, Tokens, TLS, Discovery oder Fernzugriff;
- Ruida-Lease-Protokoll, Queueing oder Jobübertragung;
- Proxmox-, Container- oder Systemdienst-Deployment.

## Nächste Schritte

1. Empfangene Revisionen bei Charon bestätigen und nur unbestätigte Stände
   wiederholt zustellen.
2. Inbox-Übersicht und sicheren Übernehmen-/Später-Ablauf ergänzen.
3. Push-Kanal und Konfliktbenachrichtigung ergänzen; zunächst ganze Version
   übernehmen oder zurückstellen. Stabil identifizierbare Shapes/Layer sind
   Voraussetzung für späteren Vergleich und Drei-Wege-Objekt-Merge.
4. Arbeitsplatzbezogene Settings- und Laserprofil-Sicherungen ergänzen.
5. Explizites `Verbinden`/`Trennen` im Laser-Tab einführen.
6. Ruida-Lease, Heartbeat, Übergabe-Push und sichere Zwangsfreigabe als eigenen
   Meilenstein umsetzen.

## Umsetzungsstand

Der erste Meilenstein ist umgesetzt:

- Charon bindet standardmäßig und erzwungen an `127.0.0.1:3737`;
- Health und Handshake antworten mit JSON und wurden gegen einen real
  gestarteten lokalen Prozess geprüft;
- der Client liegt in `luxifer-application`, validiert URL, HTTP-Status,
  Serverkennung und Protokollversion und übersetzt Fehler in `AppError`;
- Aktivierung, URL und Verbindungstest liegen in der globalen
  Charon-Einstellungssektion; alte Settings erhalten sichere Defaults.
- jeder Datenbereich erhält beim ersten Start eine persistierte
  `workplace_id`; der sichtbare Arbeitsplatzname bleibt frei änderbar;
- der Verbindungstest registriert den Arbeitsplatz und zeigt Charons bekannten
  Anwesenheits-Snapshot;
- bei aktivierter Charon-Koordination meldet sich LuxiFer alle fünf Sekunden
  aus einem Hintergrundthread. Netzwerkfehler blockieren den UI-Thread nicht
  und werden als getrennter Zustand sichtbar; nach 15 Sekunden ohne Meldung
  gilt ein Arbeitsplatz als offline;
- Charon hält die Registrierung vorerst nur im Arbeitsspeicher. Ein Neustart
  leert die Anwesenheitsliste, die laufenden Clients melden sich selbstständig
  wieder an;
- `scripts/run-local-charon-demo.sh` startet Charon, Office und Workshop mit
  voneinander isolierten Datenverzeichnissen in drei Terminals.
- nach jedem erfolgreichen lokalen Speichern legt LuxiFer bei aktiviertem
  Charon einen atomar geschriebenen Outbox-Eintrag unter
  `sync/outbox/<revision_id>/` an. Manifest und eigene `payload.luxi`-Kopie
  bleiben auch bei einem späteren Strg+S unverändert;
- Sync-Revisionen sind von den sichtbaren Projektversionen getrennt. Sie tragen
  Projekt-/Versions-/Arbeitsplatz-ID, Elternrevision, Zeitpunkt, Inhaltshash
  und Status. Dadurch bildet auch mehrfaches Speichern innerhalb etwa V1 eine
  eindeutige, konfliktfähige Kette;
- ein Outbox-Fehler macht das zuvor erfolgreiche lokale Speichern nicht
  rückgängig und wird als separate Warnung angezeigt.
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
  ersten Netzwerkblock. Assets sind weiterhin nicht Bestandteil des Transfers.
- Charon liefert einem Arbeitsplatz ausschließlich Revisionen anderer
  Arbeitsplätze. LuxiFer prüft deren Hash und legt sie idempotent und atomar
  unter `sync/inbox/<revision_id>/` ab;
- Inbox-Einträge starten mit `pending_review`. Empfangene Payloads verändern
  weder den Canvas noch lokale Projektdateien automatisch;
- neue Inbox-Einträge werden per Toast gemeldet. Bis zur serverseitigen
  Empfangsbestätigung liefert Charon bekannte Revisionen erneut; die lokale
  idempotente Ablage verhindert dabei Duplikate.

Noch offen sind Inbox-Bestätigung/-Übernahme, Settings-Transfer, Push-Kanal,
Konfliktvergleich sowie Ruida-Leases. Charon darf Versionen verteilen und
Verbindungen koordinieren, aber keine Projektinhalte selbst bearbeiten oder
laufende Jobs unterbrechen.
