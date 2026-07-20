# ADR 0021: Zusatzachsen Z/U, Jog-Modell und Rotary

- Status: Vorgeschlagen
- Datum: 2026-07-20
- Betrifft: Laserpanel, Laserprofile, Ruida-Treiber, Maschinenbewegung
  (Jog/Dauerlauf), Positionsanzeige, spätere Rotary-Gravur

## Kontext

Auf dem Branch `feature/ruida-u-achse-jog` wurde an echter Hardware
(RDC6445G) verifiziert, wie sich die Zusatzachsen Z (Fokus/Betthöhe) und U
(Rotary/Drehachse) über das Ruida-Protokoll ansteuern lassen. Verifiziert und
byte-genau durch Tests gepinnt sind:

- **Schritt-Move** einer Achse: `D9 <achse> 02 <coord>` (relativ interpretiert),
  Achs-Byte X=0/Y=1/Z=2/U=3.
- **Dauerlauf** (Halten): `D9 D8 <flags>` mit
  `flags = 0x20 | (achse << 1) | richtung | stop`, alle vier Achsen in beiden
  Richtungen aufgezeichnet.
- **Positionsregister** X=`0x0421`, Y=`0x0431`, Z=`0x0441`, U=`0x0451`.
- Ein Kodierungsfehler in `encode_coord` für negative Koordinaten wurde
  nebenbei gefunden und behoben (32-Bit-Zweierkomplement statt 35-Bit-Maske).

Die Machbarkeit ist damit geklärt. Der Prototyp auf dem Branch ist aber
bewusst **nicht** für `main` gedacht, weil beim Ausprobieren mehrere
Design-Schulden entstanden sind:

1. **Keine Achsen-Verfügbarkeit.** Die Z/U-Bedienelemente sind immer aktiv,
   auch wenn die Maschine gar keine Z- oder U-Achse angeschlossen/aktiviert
   hat. Solche Klicks senden Kommandos ins Leere bzw. an nicht vorhandene
   Achsen.

2. **Zwei Kommando-Pfade mit widersprüchlicher Richtung.** Schritt-Move
   (`D9 <achse> 02`) und Dauerlauf (`D9 D8`) sind bei Ruida pro Achse
   gegeneinander invertiert. Der Prototyp gleicht das mit zwei ad-hoc-Helfern
   (`hold_dir`, `step_dir`) aus, die pro Achse einen der beiden Pfade drehen.
   Das ist fragil: „Schritt" und „Dauer" sind zwei verschiedene Prozesse mit
   eigener Richtungslogik, obwohl es fachlich **eine** Richtung „Achse +/−"
   gibt.

3. **Keine konfigurierbare Inversion.** Ob „U +" im oder gegen den
   Uhrzeigersinn dreht bzw. ob „Z +" hoch oder runter fährt, hängt von Verkabelung
   und Betrachtungsseite ab. Der Prototyp kodiert eine feste Konvention; es gibt
   keine nutzerseitige Umkehrung pro Achse.

4. **Rotary nur über U betrachtet.** Viele Ruida-Maschinen ohne U-Firmware
   betreiben die Rotary klassisch über die **Y-Achse** (Y-Motor abgeklemmt,
   Rotary an den Y-Ausgang). Dieser Fall wurde bisher nicht mitgedacht.

## Entscheidung

**(A) Achsen-Verfügbarkeit wird aus dem Controller gelesen. (B) Jog hat EIN
Richtungsmodell; „Tippen" und „Halten" sind nur zwei Auslöse-Arten desselben
Achsen-Jog. (C) Achsen-Inversion ist pro Achse im Laserprofil konfigurierbar.
(D) Rotary über Y wird als eigener Betriebsmodus des Profils unterstützt.**

### (A) Achsen-Verfügbarkeit aus den Vendor-Settings

Studio leitet die vorhandenen Zusatzachsen aus dem Controller ab, nicht aus
manueller Eingabe. **Wichtig — zwei getrennte Rotary-Mechanismen** (recherchiert,
RDWorks-Doku und -Foren):

- **Rotary über Y** (klassisch): Der vorhandene Settings-Block (`settings.rs`,
  Gruppe „Rotary") liest bereits `0x0226` **rotary_enable** (`bit_mask:
  Some(1)`), `0x021F` **pulses_per_rot** („circle pulse", Mikroschritte pro
  Umdrehung) und `0x0221` **rotary_diameter**. `rotary_enable` schaltet den
  Controller in den Y-Rotary-Modus: die **Y-Bewegung wird zur Drehung** (der
  Y-Motor ist dann typischerweise abgeklemmt und die Rotary an den Y-Ausgang
  gesteckt). Diese Register betreffen **nicht** die U-Achse.

- **Rotary über U** (Firmware-Patch): Eine eigenständige 4. Achse mit eigenen
  Move-Kommandos (`D9 03 …`, Positionsregister `0x0451`) — das, was auf dem
  Branch gejoggt wurde. `0x0226` sagt darüber nichts aus.

Daraus folgt für die Verfügbarkeit:

- **Y-Rotary** ist über `0x0226 rotary_enable` sicher erkennbar (Bit bekannt).
- **U-Achse** hat noch **keine** sicher identifizierte Enable-Quelle. Kandidat
  ist das Achs-Control-Register `0x0050` (U); sein Enable-Bit ist noch zu
  dekodieren. Bis dahin liefert der Treiber `has_u_axis` konservativ (`false`).
- **Z-Achse**: analog, Kandidat `0x0040`, Enable-Bit noch zu dekodieren →
  `has_z_axis` konservativ `false`.

Der Ruida-Treiber meldet die Verfügbarkeit geräteneutral über die
`DriverCapabilities`:

```
pub struct DriverCapabilities {
    …
    pub has_z_axis: bool,      // aus 0x0040 (Enable-Bit noch zu dekodieren)
    pub has_u_axis: bool,      // aus 0x0050 (Enable-Bit noch zu dekodieren)
    pub rotary_on_y: bool,     // aus 0x0226 rotary_enable (bekannt)
}
```

Die Pulse/Umdrehung und der Durchmesser (`0x021F`/`0x0221`) sind für den Jog
irrelevant, aber die Grundlage der späteren Rotary-**Gravur** über Y (siehe (D)
und das Gravur-ADR) — sie müssen dort **nicht neu erfunden** werden, der
Controller hält sie bereits.

Solange nicht verbunden, sind Zusatzachsen gesperrt (wie schon Jog/Home). Die
UI (Jog-Kreuz-Ecken für Z/U) ist genau dann klickbar, wenn verbunden **und**
die jeweilige Capability `true` ist — analog zum bestehenden Muster, das
`position_read`/`user_origin_read` bereits für andere Bedienelemente nutzt.

### (B) Ein Jog-Prozess, ein Richtungsmodell

Es gibt fachlich **eine** Achsenbewegung mit **einer** Richtung „vorwärts/
rückwärts". Das Auslösen kennt zwei Arten:

- **Tippen** → fahre um einen festen Schritt (mm).
- **Halten** → fahre, solange gehalten wird (Watchdog stoppt beim Loslassen).

Der Treiber bekommt genau diese Semantik, z. B.:

```
enum JogMotion { Step(f64 /* mm */), HoldStart, HoldStop }
fn jog(&self, axis: MachineAxis, dir: AxisDir, motion: JogMotion, speed) -> …
```

Die pro-Achse-Invertierung zwischen `D9 <achse> 02` und `D9 D8` wird **im
Treiber** einmal aufgelöst (dort, wo die zwei Ruida-Kommandos gebaut werden),
nicht in der UI. Nach außen (Application, UI) existiert nur die eine logische
Richtung. Die Helfer `hold_dir`/`step_dir` aus dem Prototyp entfallen ersatzlos;
die UI meldet nur „Achse X, Richtung +, Tippen/Halten".

### (C) Konfigurierbare Inversion pro Achse

Das Laserprofil bekommt je Achse ein Invertierungs-Flag:

```
pub struct AxisConfig {         // im LaserProfile, serde(default)
    pub invert_z: bool,
    pub invert_u: bool,
    …
}
```

Die Inversion greift an genau einer Stelle im Treiber (Richtung → Achs-Kommando),
sodass sie Tippen und Halten gleichermaßen betrifft — es kann keine Divergenz
zwischen den Modi mehr geben. Damit beantwortet sich auch die offene
U-Rotationsfrage: Die „richtige" Drehrichtung ist Verkabelungs-/Betrachtungs-
sache und wird pro Maschine im Profil festgelegt, nicht hartkodiert.

### (D) Rotary über Y

Das Profil bekommt einen Rotary-Betriebsmodus:

```
pub enum RotaryMode {
    Aus,
    UAchse,   // Rotary am U-Ausgang (Firmware-Patch), Y bleibt Gantry
    YAchse,   // Rotary am Y-Ausgang, Y-Motor abgeklemmt (klassisch)
}
```

- **`UAchse`**: wie auf diesem Branch — U ist eine eigenständige Achse; Jog und
  (später) Gravur nutzen U zusätzlich zur X/Y-Ebene.
- **`YAchse`**: Rotary läuft über den Y-Ausgang; im Controller ist
  `0x0226 rotary_enable` gesetzt. Der Controller behandelt die Y-Bewegung
  selbst als Drehung und skaliert sie über seine `pulses_per_rot`/`diameter`
  (`0x021F`/`0x0221`). Für den **Jog** heißt das: die U-Bedienelemente
  entfallen, die Y-Pfeile *sind* die Drehung — es braucht **keine** app-seitige
  Skalierung, der Controller macht sie. Für die **Gravur** ist damit im
  einfachsten Fall nichts Besonderes zu tun: ein normaler Y-Job wird vom
  Controller als Rotary interpretiert, solange `rotary_enable` an ist (Details
  klärt das Gravur-ADR).

Dieses ADR legt nur den **Modus im Profil und sein Jog-Verhalten** fest. Die
eigentliche Rotary-**Gravur** ist Gegenstand eines eigenen späteren ADR. Der
Y-Fall wird hier nicht verbaut, und die Controller-Register (`0x0226`/`0x021F`/
`0x0221`) sind bereits im Settings-System verfügbar.

## Konsequenzen

**Positiv**

- Bedienelemente sind nur aktiv, wenn die Achse real vorhanden ist — keine
  Kommandos an nicht existierende Achsen.
- Eine einzige Richtungslogik statt zweier widersprüchlicher Pfade; die
  Modus-Divergenz (Tippen ≠ Halten) ist strukturell ausgeschlossen.
- Inversion und Rotary-Modus sind pro Maschine konfigurierbar und persistent
  (Laserprofil, wie Bett/Nullpunkte).
- Der klassische Rotary-über-Y-Betrieb bleibt möglich.

**Aufwand / Risiko**

- Y-Rotary-Verfügbarkeit ist bereits abgedeckt (`0x0226 rotary_enable`, Bit
  bekannt). Die **U**- und **Z**-Enable-Bits (`0x0050`/`0x0040`) müssen noch an
  Hardware dekodiert werden (Register mit/ohne angeschlossener Achse
  vergleichen); bis dahin `has_u_axis`/`has_z_axis` konservativ gesperrt.
- `LaserProfile` und `DriverCapabilities` wachsen um Felder; die Serialisierung
  bleibt über `serde(default)` rückwärtskompatibel (bestehende Profile laden
  weiter, Zusatzachsen zunächst aus/gesperrt).
- Das Trait `MachineDriver` wird auf das einheitliche `jog(axis, dir, motion,
  speed)` umgestellt; die Prototyp-Methoden (`jog_axis`, `hold_axis_start/stop`)
  werden dabei zusammengeführt.

## Umsetzungsreihenfolge (nach Annahme des ADR)

1. `DriverCapabilities` um `has_z_axis`/`has_u_axis`/`rotary_on_y`; Ruida liest
   `0x0226` (Y-Rotary, bekannt) und — sobald dekodiert — `0x0050` (U) / `0x0040`
   (Z); UI sperrt Z/U-Ecken entsprechend.
2. Trait auf `jog(axis, dir, motion, speed)` vereinheitlichen; Inversion +
   Kommando-Auswahl in den Treiber ziehen; `hold_dir`/`step_dir` entfernen.
3. `AxisConfig` (Inversion) ins `LaserProfile` + Bedienung in der
   Laser-Verwaltung.
4. `RotaryMode` ins `LaserProfile`; Jog-Verhalten für `YAchse` (U-Elemente aus,
   Y ist die Drehung).
5. Die Rotary-**Gravur** bleibt einem eigenen ADR vorbehalten.

## Nicht Teil dieser Entscheidung

- Die Rotary-Gravur selbst (Y-Skalierung, Durchmesser, Schritte/Umdrehung).
- Z-Positionsregister-Semantik: `0x0441` zeigte im Test zeitweise einen festen
  Wert (3200) statt der Ist-Position; ob es die echte Z-Position ist, ist noch
  offen und für die Anzeige gesondert zu klären.
