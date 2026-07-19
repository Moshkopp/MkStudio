# Charon betreiben

Charon bleibt standardmäßig ausschließlich lokal erreichbar:

```bash
cargo run -p charon
```

## Internes Netzwerk / Proxmox

Eine Netzwerkbindung ist eine ausdrückliche Betriebsentscheidung. Charon hat
aktuell keine Benutzeranmeldung und kein TLS. Port `3737/tcp` darf deshalb nur
aus einem vertrauenswürdigen internen Netz erreichbar sein und darf nicht ins
Internet weitergeleitet werden.

Zum Test in einer Proxmox-VM oder einem LXC-Container:

```bash
CHARON_BIND=0.0.0.0:3737 \
CHARON_ALLOW_NETWORK=1 \
CHARON_DATA_DIR=/var/lib/charon \
./charon
```

`0.0.0.0` lauscht auf allen IPv4-Interfaces. LuxiFer verwendet als Charon-URL
die konkrete interne Adresse des Gasts, beispielsweise
`http://192.168.10.25:3737`.

Vor dem Einrichten des Dienstes kann die Erreichbarkeit von einem anderen
Rechner geprüft werden:

```bash
curl http://192.168.10.25:3737/health
curl http://192.168.10.25:3737/api/v1/handshake
```

## Installation als systemd-Dienst

Im Repository wird zuerst das Release-Binary gebaut und anschließend das
Installscript als root ausgeführt:

```bash
cargo build -p charon --release
sudo ./scripts/install-charon.sh
```

Das Script ist wiederholt ausführbar und aktualisiert eine bestehende
Installation. Es richtet Folgendes ein:

- Systembenutzer und -gruppe `charon`;
- Binary unter `/usr/local/bin/charon`;
- persistente Daten unter `/var/lib/charon`;
- Konfiguration unter `/etc/charon/charon.env`;
- gehärtete systemd-Unit `charon.service`.

Abweichende Adressen, Datenpfade oder ein separat übertragenes Binary können
explizit angegeben werden:

```bash
sudo ./scripts/install-charon.sh \
  --binary ./charon \
  --bind 192.168.10.25:3737 \
  --data-dir /srv/charon
```

Mit `--no-start` wird der Dienst installiert und aktiviert, aber noch nicht
gestartet. Das Script verändert absichtlich keine Firewallregeln.

Die Proxmox- oder Gast-Firewall sollte `3737/tcp` auf das tatsächlich genutzte
interne Subnetz beziehungsweise die LuxiFer-Arbeitsplätze begrenzen.

## Update eines bestehenden Proxmox-Dienstes

Nach dem einmaligen Clone wird Charon direkt aus dem Repository aktualisiert:

```bash
cd /opt/LuxiFer
./update.sh
```

Das Script wird als normaler Benutzer ausgeführt und fragt für Installation
und Dienstneustart selbst nach `sudo`. Es akzeptiert nur einen sauberen
Git-Stand, zieht den aktuellen Upstream per Fast-Forward, testet und baut
Charon und ersetzt anschließend `/usr/local/bin/charon`. Startet die neue
Version nicht, wird automatisch `/usr/local/bin/charon.previous`
wiederhergestellt. `/var/lib/charon` und `/etc/charon/charon.env` werden beim
Update nicht verändert.

Arbeitsplatzsicherungen werden versioniert und nur bei geändertem Inhalt neu
angelegt. Charon behält pro Arbeitsplatz und Sicherungstyp die letzten zehn
Änderungen, danach je einen Tagesstand für 30 Tage und anschließend je einen
30-Tage-Stand für zwölf Zeiträume. Bestehende einzelne Sicherungsdateien aus
Protokollversion 2 bleiben lesbar und gehen in diese Aufbewahrung ein.

Seit Protokollversion 3 werden Laser- und Materialprofile zusätzlich als
gemeinsamer Katalog automatisch zwischen allen Arbeitsplätzen abgeglichen.
Änderungen verwenden Inhaltshashes und Basisrevisionen; Löschungen bleiben als
Tombstones erhalten. Aktive Auswahlen sind weiterhin rein lokal. Die
arbeitsplatzbezogene Historie dient nur noch der bewussten Wiederherstellung.
