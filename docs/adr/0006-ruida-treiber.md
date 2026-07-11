# ADR 0006: Ruida-Treiber (JobPlan → Ruida, Verbindung + Steuerung)

## Status
Akzeptiert — 2026-07-09

## Kontext

Der Core kann heute Geometrie, Layer/Farbe, Fill und einen geräteunabhängigen
**`JobPlan`** (ADR 0001) bauen; die Preview (ADR 0005) visualisiert genau diesen
Plan. Was fehlt, ist der Schritt **an die Maschine**: aus dem `JobPlan` echte
Ruida-Bytes machen und den Laser real ansteuern.

Der Ruida RDC6445G spricht **verschlüsselte Binär-Pakete über UDP** (Swizzle +
Checksumme, Ports 50200/40200). Das Protokoll ist bereits **an echter Hardware
verifiziert** in zwei Referenzen:

- **`nur zur Referenu/thorlaser_python`** — die **maßgebliche** Referenz: an
  echter HW gefahren (Cut+Fill real), der Job-Header **byte-genau gegen
  pcap-Captures** (`twolayer.pcap`, `abs/cur/user_posi.pcap`) validiert. Enthält
  Ruida, GRBL **und** Marlin nebeneinander (`controllers/`), zeigt also die
  geräteneutrale Grenze direkt.
- `nur zur Referenu/ThorBurn/…/hardware/` (Rust) — jüngere Teilportierung.

Beide werden **analysiert und im aktuellen Stil neu implementiert, nicht
kopiert** (CLAUDE.md §6); eigene Ping-Tests bestätigen den Transport (Memory
`ruida-transport`).

ADR 0001 hat die Architektur schon festgelegt: Der Core erzeugt den `JobPlan`
und definiert den **`MachineDriver`-Trait**; die Treiber sind **eigene Crates**,
die den Plan in ihr Format übersetzen. Ruida ist damit **nur einer von mehreren**
Treibern (GRBL/miniGRBL folgen) und darf die Kernlogik nicht prägen.

**Wichtiger Unterschied zur Referenz.** Bei ThorBurn lag die Layer-Dedup-Logik
(gleiche Speed/Power → ein Ruida-Layer-Index) **im Compiler** und war mit der
Geometrie verwoben. Bei uns liefert der `JobPlan` bereits fertige `JobLayer`
mit Parametern und Arbeit (`Cut`/`Fill`). Der Ruida-Treiber wird dadurch
**deutlich dünner**: er mappt `JobLayer` 1:1 auf Ruida-Layer-Indizes und
serialisiert. Keine Fachlogik im Treiber.

## Entscheidung

**Ein eigenes Crate `luxifer/drivers/ruida` implementiert `MachineDriver`:
Job-Kompilierung (`JobPlan` → Ruida-Bytes) *und* Live-Steuerung über UDP.
Beides lebt im Treiber; der Core bleibt gerätefrei.**

### 1. Der `MachineDriver`-Trait wächst um die Live-Steuerung

ADR 0001 hat `name()` + `compile()` definiert. Für den realen Betrieb kommt die
Live-Steuerung dazu — als Trait-Methoden im Core, damit die GUI **ausschließlich
über den Trait** spricht und nie Ruida-Details kennt:

```rust
pub trait MachineDriver {
    fn name(&self) -> &str;

    // Kompilierung (ADR 0001) — geräteunabhängiger Plan → Geräte-Bytes.
    fn compile(&self, plan: &JobPlan, layers: &[Layer]) -> Result<Vec<u8>, String>;

    // Live-Steuerung (dieser ADR). Alle Maße in mm bzw. mm/s.
    fn connect(&mut self, ip: &str) -> Result<(), DriverError>;
    fn status(&self) -> Result<MachineStatus, DriverError>;
    fn jog(&self, x_mm: f64, y_mm: f64, speed_mm_s: f64) -> Result<(), DriverError>;
    fn home(&self, speed_mm_s: f64) -> Result<(), DriverError>;
    fn go_origin(&self, speed_mm_s: f64) -> Result<(), DriverError>;
    fn frame(&self, plan: &JobPlan, speed_mm_s: f64, mode: StartMode) -> Result<(), DriverError>;
    fn send_job(&self, bytes: &[u8]) -> Result<(), DriverError>;
    fn stop(&self) -> Result<(), DriverError>;
}
```

`MachineStatus` (`is_running`, `is_paused`, `pos_x_mm`, `pos_y_mm`),
`DriverError` und `StartMode` (Absolut / AktuellePosition / Benutzerursprung)
gehören in den Core — sie sind gerätefreie Vokabeln, kein Ruida-Detail.

**Der Treiber ist eine konfigurierte Maschine — Profil bei der Erzeugung.**
`RuidaDriver::new(profil)` bekommt das Laser-Profil (ADR 0007: IP/Port,
Bettgröße, Scan-Offset-Kurve) **einmal beim Erstellen** und trägt es als
Instanz-Zustand. `compile`/`jog`/`frame`/… nehmen das Profil **nicht** erneut als
Parameter — die Kalibrierung gehört zur Maschine, nicht zu jedem Aufruf (so auch
die Python-Referenz: der Controller hält `scanning_offset` als eigenen Zustand).
Ändert der Nutzer das aktive Profil, wird der Treiber **neu erzeugt** — das ist
der saubere Umschaltpunkt (s. ADR 0007: Laser-Dropdown im Panel).

### 2. Kompilierung: `JobPlan` → Ruida-Job

Der Ruida-Job hat einen festen, an echter HW verifizierten Aufbau (Referenz
`ruida_compiler/`), den wir übernehmen. Reihenfolge:

1. **Preamble** — Startmodus (`D8 10/11/12`), Arbeitsbereich-BBox (`E7 03/07`,
   `E7 50/51`).
2. **Layer-Config** — pro Ruida-Layer: Speed (`C9 04`), Min/Max-Power
   (`C6 31/32/41/42`), Farbe als **BGR** (`CA 06`), Layer-BBox (`E7 52/53/61/62`).
3. **F-Block + 2. BBox-Satz** (`F1/F2`-Blöcke, `E7 13/17/23/37`).
4. **Geometrie-Body** — pro Layer ein Settings-Block (`CA 01/02`, Speed `C9 02`,
   Power `C6 01/02/21/22`, Cut/Scan-Umschaltung), dann die Pfade:
   - **`LayerWork::Cut`**: `88`/`A8` (Move/Cut absolut, µm) bzw. die relativen
     `8A/8B/A9/AA/AB`-Kurzformen; lange Strecken über die `*_long`-Splitter
     (±8191 µm-Grenze).
   - **`LayerWork::Fill`**: die Scanline-Segmente des `JobPlan` als
     Zeilen-Muster (Move an Zeilenanfang, Cut bis Zeilenende), **bidirektional**
     (Zeilen abwechselnd vor-/rückwärts) — und hier greift der Scan-Offset (s.
     §6).
5. **Trailer** — `EB E7 00`, Ursprungs-BBox (`DA 01 06 20`), dann
   **Datei-Prüfsumme** (`recompute_file_sum`) und `D7` (EOF).

**Zahlen-Kodierung** (reine Funktionen, testbar, ohne I/O): 7-Bit-pro-Byte
big-endian (`encode_value`/`decode_value`), 5-Byte-µm-Koordinaten
(`encode_coord`, 35-Bit-Maske), 14-Bit-Power (`encode_power`), µm/s-Speed
(`encode_speed`). Swizzle (`^`-Kaskade + `magic 0x88` + `+1`) und 16-Bit-Paket-
Checksumme über die geswizzelten Bytes.

Was der `JobPlan` **nicht** liefert und der Treiber daher **nicht** macht:
Fill-Berechnung (liegt im Core), Layer-Dedup (der Plan ist schon dedupliziert),
Rotation (im Plan bereits eingerechnet). Der Treiber **serialisiert nur**.

### 3. Transport: UDP im Treiber-Crate

Der komplette UDP-Stack lebt in `luxifer/drivers/ruida` — der Core kennt kein
Socket, keinen Port, keinen Swizzle:

- Senden → **50200**, Empfangen ← **40200** (lokal auf 40200 binden — sonst
  verpasst man die Antwort; hardware-verifiziert, Memory `ruida-transport`).
- Payload wird in **≤1024-Byte-Chunks** gesendet, jeder Chunk bekommt ein
  eigenes **ACK** (`0xCC`) / **NAK** (`0xCF`), mit Retry.
- `connect()` prüft per **Ping** echte Erreichbarkeit (UDP ist verbindungslos;
  ein offener Socket allein sagt nichts).
- Register-Abfragen (`DA 00 <addr>`) für Status/Position/Ursprung, Antwort
  `DA 01 …` wird auf die passende Adresse gematcht (versetzte Pakete überlesen).

Der Transport läuft **nie im GUI-/Tauri-Command-Thread blockierend** so, dass er
die Oberfläche einfriert — langlaufende Sends (Job-Upload) laufen in einem
Hintergrund-Thread; der Command kehrt sofort zurück und meldet Fortschritt/Status
per Event. (Feinschliff der Nebenläufigkeit ist Umsetzungsdetail, nicht Teil
dieser Entscheidung.)

### 4. Crate-Struktur

```
luxifer/drivers/ruida/
  Cargo.toml          # dep: luxifer-core (JobPlan, Trait, StartMode, …)
  src/lib.rs          # RuidaDriver: impl MachineDriver
  src/protocol.rs     # Swizzle, Encoding, Opcode-Bausteine (rein, testbar)
  src/compile.rs      # JobPlan → Bytes (Preamble→Config→F→Geometrie→Trailer)
  src/transport.rs    # UDP: connect/send/query/ping, ACK/NAK, Chunking
```

`protocol.rs` und `compile.rs` sind **ohne Netzwerk testbar**: Byte-Vergleich
gegen die verifizierten Referenz-Sequenzen (z. B. Rechteck endet auf `D7`,
Swizzle-Roundtrip, Koordinaten-Roundtrip, Power-Max = `0x3FFF`).

### 5. Ruida ist vollständig gekapselt — der Plan bleibt geräteneutral

Kernanforderung: **GRBL, miniGRBL und Ruida konsumieren denselben `JobPlan`.**
Der Plan darf daher **nichts** enthalten, das für ein Gerät zugeschnitten ist —
alles Gerätespezifische lebt hinter `MachineDriver::compile`. Konkret verläuft
die Grenze so:

**Im Core (geteilte Quelle für alle Treiber):**
- Pfade als **mm in `f64`** (`Path { points: Vec<Pt>, closed }`), Fill als
  mm-`FillSegment`. Rotation ist eingerechnet, Fill berechnet, Layer
  dedupliziert. Reine Geometrie/Parametrik.
- **Reihenfolge und Fahrwege gehören in den Plan, nicht in einen Treiber** —
  weil GRBL dieselbe Optimierung braucht wie Ruida. Fahrweg-Optimierung ist
  also Core-Arbeit (späterer Schritt), **niemals** „für Ruida optimiert".

**Im Ruida-Treiber (und nur dort):**
- Umrechnung mm→**µm-Integer**, das 5-Byte-Encoding, die **±8191-µm-Grenze** der
  Relativ-Befehle und das Aufsplitten langer Strecken (`*_long`). Das ist reine
  Ruida-Zahlendarstellung — sie darf die Punkte im Plan **nicht** vorformen
  (kein „schon in µm", kein Vor-Splitten im Core).
- Swizzle, Opcodes, Paket-Checksumme, Farbe-als-BGR, Layer-Index-Zuweisung,
  Preamble/Trailer, UDP.

**Prüffrage bei jeder Ergänzung:** Würde GRBL dieselbe Eingabe genauso gut
gebrauchen? Wenn nein, ist sie im Plan falsch platziert und gehört in den
Treiber. Der Beweis, dass die Kapselung hält, ist konkret: derselbe `JobPlan`
muss sich später ohne Änderung durch einen GRBL-Treiber schicken lassen.

### 6. Scan-Offset (Reversal-Korrektur) — im Treiber, nicht im Plan

Beim bidirektionalen Rastern brennen die **Rückwärts-Zeilen** durch die
mechanisch/optische Latenz des Kopfes horizontal versetzt gegen die
**Vorwärts-Zeilen** — der Rand franst aus. Der Versatz ist
**geschwindigkeitsabhängig** und ein **physikalischer Kennwert der Maschine**.
Er ist damit exakt der Fall aus §5: **Gerätekorrektur, kein Job-Inhalt.**

Deshalb:
- Die **Offset-Kurve** (Tabelle Geschwindigkeit → Offset, interpoliert) ist ein
  **Feld des Laser-Profils** (`ScanOffset`, ADR 0007), kein Teil des `JobPlan`.
- Die **Anwendung** macht **der Ruida-Treiber** beim Serialisieren des Fill:
  Vorwärts-Zeile `+offset`, Rückwärts-Zeile `−offset`, wobei `offset` aus der
  Profil-Kurve zur Layer-Geschwindigkeit interpoliert wird (`interpolate_scan_offset`,
  µm). Der `JobPlan` bleibt Ideal-Soll-Geometrie ohne jede Korrektur.

Konsequenz für die Signatur: Der Treiber braucht beim Kompilieren Zugriff auf die
Kalibrierung seines Profils. `compile` bekommt daher den relevanten Profil-Teil
mit (bzw. der Treiber hält ihn als Instanz-Zustand) — der **Core** reicht die
Kurve nur durch, **wendet sie nie selbst an**. Ein GRBL-Treiber, der das Problem
nicht hat, ignoriert seinen Offset schlicht.

Die **Preview** (ADR 0005) zeigt weiterhin die **ideale Soll-Geometrie** — die
Reversal-Korrektur ist Sub-mm-Maschinenphysik und keine sinnvolle Anzeige.

**Overscan macht beim Ruida der Controller.** Das Überfahren des Zeilenrands
(damit der Kopf auf voller Geschwindigkeit brennt) erledigt der Ruida-Controller
**selbst, intern und nicht beeinflussbar**. Der **Ruida-Treiber** emittiert daher
keine Overscan-Wege — für ihn ist nur der geschwindigkeitsabhängige Scan-Offset
zu tun. Ob ein **GRBL-Treiber** Overscan selbst erzeugen muss, ist offen und
**Thema des GRBL-Treibers** (dessen Controller macht es evtl. nicht) — nicht Teil
dieser Ruida-Entscheidung und ausdrücklich nicht generell ausgeschlossen.

### 7. Aus der bewährten Python-Referenz geklärt (thorlaser_python)

Die an echter HW gefahrene Python-Version (`nur zur Referenu/thorlaser_python`,
byte-genau gegen pcap-Captures validiert) klärt drei Punkte, die sonst beim Bauen
aufschlagen:

- **`air_assist` und `bidirectional` müssen in den `JobLayer`.** Der Ruida
  emittiert Air-Assist pro Layer (`CA 01 13` an / `CA 01 12` aus); der Core-
  `Layer` hat das Feld schon, `JobLayer` (in `job.rs`) trägt es noch **nicht**.
  Beide Felder sind geräteneutral (auch GRBL kennt Luft und Scan-Richtung) und
  **werden zu `JobLayer` ergänzt** (`air_assist: bool`, `bidirectional: bool`).
  `bidirectional` steuert den Rückwärts-Scan im Fill (nur dann greift der
  Scan-Offset §6); bei `false` fährt jede Zeile in gleicher Richtung.
- **Kein `mode`-Feld im `JobLayer`.** Der Modus steckt implizit in der
  `LayerWork`-Variante (`Cut` = Kontur, `Fill` = Fläche); der Treiber leitet das
  Raster-Gating daraus ab (nächster Punkt). Kein zweites `mode`-Feld — es gäbe
  sonst zwei zu synchronisierende Wahrheiten. `Raster`/`Image` werden erst mit
  dem Bild-Job eigene `LayerWork`-Varianten.
- **`passes` wird byte-transparent in die Geometrie kompiliert.** Der ursprüngliche
  Plan (passes rein auf Ausführungs-Ebene, Job n-mal senden) wurde nie umgesetzt:
  `send_job` kennt `passes` nicht und fuhr den Job immer nur einmal — egal welcher
  Wert eingestellt war. Die verifizierte Referenz (ThorBurn, `ruida_compiler/
  geometry.rs`) macht es richtig: der Settings-Block steht **einmal**, die
  Fahrwege dahinter werden **n-mal** wiederholt (Schleife `for _ in 0..passes`
  um Cut/Fill/Raster). Der kompilierte Job ist damit bei mehr Passes länger;
  der Controller fährt die Kontur tatsächlich mehrfach. `passes` bleibt trotzdem
  geräteneutral im `JobLayer` — jeder Treiber setzt es auf seine Art um (GRBL
  wiederholt den G-Code, Ruida die Byte-Geometrie).
- **Raster-Gating ist Ruida-Sache, gesteuert vom `LayerMode`.** Fill/Image
  brauchen `CA 01 01` (Laser feuert nur bei X-Fahrt), Cut `CA 01 00`. Die
  Umschaltung leitet der Treiber aus `LayerWork`/`mode` ab — der Plan sagt nur
  *was* (Cut vs. Fill), nicht *wie gegatet*.

## Invarianten

1. **`luxifer-core` DARF NICHT** Ruida-Bytes, Ports, Swizzle oder Sockets
   enthalten. Es definiert nur `JobPlan`, `MachineDriver`, `MachineStatus`,
   `DriverError`, `StartMode`.
2. Der **`JobPlan` ist die einzige Eingabe** der Kompilierung. Neue Arbeit
   (Raster) erweitert `LayerWork` im Core — der Treiber bekommt dann ein neues
   `match`-Arm, aber keine neue Fachlogik.
3. Der Treiber **berechnet keine Geometrie/Fill/Reihenfolge** — das kommt fertig
   aus dem Plan. Er kodiert und sendet.
4. **Der `JobPlan` ist geräteneutral: mm in `f64`, keine Geräteform.** Kein µm,
   keine Bytes, keine ±8191-Grenze, kein vorgesplitteter Pfad — solche
   Zuschnitte macht **ausschließlich der Treiber**. Derselbe Plan muss ohne
   Änderung auch einen GRBL-Treiber speisen können; das ist der Prüfstein.
5. Die **GUI spricht nur über `MachineDriver`** — nie direkt mit `transport`
   oder `protocol`.
6. Byte-Encoding und Job-Aufbau werden **gegen die verifizierten Referenz-
   Sequenzen getestet**, nicht „auf gut Glück" geschrieben.
7. **`passes` wird n-fach in die Geometrie kompiliert.** Der Settings-Block je
   Layer steht einmal, die Fahrwege dahinter n-mal (`for _ in 0..passes`). Nur so
   fährt der Controller die Kontur mehrfach — ein einzelner Sende-Vorgang kennt
   `passes` nicht. `passes` bleibt geräteneutral im Plan; jeder Treiber setzt die
   Wiederholung selbst um.

## Konsequenzen

- LuxiFer kann erstmals einen **echten Ruida-Job erzeugen und senden** — und
  über `frame`/`jog`/`home` die Maschine vor dem Brennen ausrichten.
- Der `JobPlan` bekommt seine **zweite Konsumentin** (nach der Preview) und
  beweist damit den Nutzen von ADR 0001: Preview und echter Job teilen sich
  dieselbe Wahrheit.
- Ein `.rd`-Datei-Export fällt praktisch ab: `compile()` liefert genau die
  Bytes, die auch per UDP gehen — Speichern als Datei ist derselbe Puffer.
- GRBL/miniGRBL sind später **dasselbe Muster**: neues Crate, `MachineDriver`
  implementieren, kein bestehender Code muss geändert werden.

## Reihenfolge der Umsetzung

1. Trait im Core erweitern (`MachineStatus`, `DriverError`, `StartMode`,
   Live-Methoden). Core bleibt gerätefrei, kompiliert weiter.
2. `protocol.rs` + Tests (Swizzle-, Coord-, Power-Roundtrips) — ohne Netz.
3. `compile.rs`: `JobPlan` → Bytes, Cut zuerst, dann Fill. Byte-Tests gegen
   Referenz-Sequenzen.
4. `transport.rs`: connect/ping/send/query. Erst an echter HW verifizieren
   (Release, Memory `performance-release-messen`).
5. GUI-Anbindung (Tauri-Command „senden"/„rahmen"/„stop") — eigener Schritt.

## Nicht Teil dieser Entscheidung

- **Raster** (Bild-Zeilen): erst wenn `LayerWork::Raster` im `JobPlan` steht
  (an ADR 0004 §5 gekoppelt).
- **Z-Achse / Fokustest** (relative `80 03`-Moves): die Referenz kann es, aber
  es hängt an einem Z-Modell im Core — späterer ADR.
- **Maschinen-Settings lesen/schreiben** (`settings_registry`): eigener Schritt.
- **GRBL/miniGRBL**-Treiber.
- Feinschliff der **Nebenläufigkeit** (Fortschritts-Events, Abbruch während
  Upload) und der GUI-Verbindungs-UX.
