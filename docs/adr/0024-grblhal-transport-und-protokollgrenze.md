# ADR 0024: grblHAL-Transport und Protokollgrenze

## Status

In Umsetzung — 2026-07-22.

## Kontext

Der vorhandene GRBL-Treiber kompiliert den geräteneutralen `JobPlan` bereits
zu G-Code, besitzt aber noch keinen Transport. Das Laserprofil speichert eine
serielle Verbindung vollständig als Port und Baudrate. Die bisherige
`MachineDriver::connect(&str)`-Grenze reduzierte diese Konfiguration vor dem
Treiber auf den Portnamen und verlor damit die Baudrate.

grblHAL kann dasselbe GRBL-Protokoll über verschiedene Streams anbieten. Der
erste produktive Transport ist USB-Serial; Ethernet soll später ergänzt werden,
ohne Parser, Streamingregeln oder GUI-Abläufe zu duplizieren.

## Entscheidung

1. `studio-core` definiert ausschließlich die geräteneutrale
   Verbindungskonfiguration und Status-/Fehlertypen. `MachineDriver::connect`
   erhält die strukturierte `Connection`, nicht einen kodierten Zielstring.
2. `driver-grbl` besitzt GRBL-Protokoll, Parser, Handshake, Flusskontrolle und
   konkrete Transporte. Der serielle Transport hält den Port während der
   gesamten Verbindung offen.
3. `studio-application` erzeugt den Treiber, koordiniert seinen Lebenszyklus,
   übersetzt Fehler und führt blockierende Abfragen außerhalb des UI-Threads
   aus. Sie kennt keine GRBL-Zeilen und keine serielle Bibliothek.
4. `studio/native` bearbeitet nur Profile und löst Application-Aktionen aus.
   Die GUI öffnet keine Ports, sendet keine GRBL-Kommandos und parst keine
   Controllerantworten.
5. Ein späterer Netzwerktransport implementiert dieselbe interne
   GRBL-Streamgrenze. Er verändert weder Core-Modell noch GUI-Workflow.

## Sicherheits- und Ablaufregeln

- Verbinden wartet auf eine gültige GRBL-Begrüßung beziehungsweise
  Identitätsantwort; ein lediglich erfolgreich geöffnetes Gerät gilt nicht als
  verbundener Controller.
- Status `?` ist ein Echtzeitkommando und wird nicht wie eine normale
  quittierte G-Code-Zeile behandelt.
- Normale Befehle werden erst nach `ok` als abgeschlossen betrachtet;
  `error:` und `ALARM:` werden typisiert an die Application gemeldet.
- Verbindungs- und Lesetimeouts sind begrenzt. Kein Geräte-I/O darf den
  Renderthread dauerhaft blockieren.
- Der erste Hardware-Smoke-Test sendet nur Identitäts-, Einstellungs- und
  Statusabfragen. Bewegung und Laserleistung folgen in getrennten Schritten.

## Abnahme

- Port und Baudrate erreichen den GRBL-Treiber unverändert.
- Ruida bleibt über dieselbe strukturierte Trait-Grenze funktionsfähig.
- Parser und Streaminglogik sind ohne Hardware deterministisch testbar.
- `/dev/ttyACM0` kann dauerhaft verbunden werden; Begrüßung, `$I` und Status
  werden erkannt, ohne Bewegungs- oder Laserkommando.
- Core und GUI besitzen keine Abhängigkeit auf das Serial-Crate.
