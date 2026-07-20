# ADR 0022: Rotary-Bauarten und treiberneutrale Achsenkalibrierung

- Status: Angenommen — Kalibrierung umgesetzt und an Hardware verifiziert;
  Rotary-Fachmodell umgesetzt, aber noch ohne Gravur-Anbindung (ADR 0023)
- Datum: 2026-07-20
- Betrifft: studio-core (Fachmodell/Rechnung), Treiber (Ruida/GRBL/FluidNC),
  Application, Laserprofile, Laserpanel
- Baut auf: ADR 0001 (Treiberabstraktion), ADR 0021 (Zusatzachsen/Jog/Rotary)

## Kontext

ADR 0021 klärt Jog und die zwei Rotary-Wege (über U bzw. über Y). Beim
Durchdenken der Rotary-**Gravur** und der Praxis fielen zwei Lücken auf, die ein
eigenes Fachmodell brauchen:

1. **Zwei Rotary-Bauarten mit unterschiedlicher Rechnung.**
   - **Chuck/Futter**: Das Objekt sitzt im Futter und dreht direkt. Der
     abzuwickelnde Umfang ergibt sich aus dem **Objektdurchmesser**
     (Umfang = π × Objekt-Ø).
   - **Roller/Abroller**: Das Objekt liegt auf zwei Walzen; die Walze schiebt
     das Objekt am Auflagepunkt. Der Vorschub hängt am **Walzendurchmesser**,
     nicht am Objektdurchmesser — das Objekt kann beliebig dick sein, eine
     Walzenumdrehung schiebt immer dieselbe Strecke. (Deckt sich mit dem
     etablierten Hinweis „circle diameter = O-ring diameter".)

   Die beiden Bauarten sind also **nicht** derselbe Fall mit anderem Wert,
   sondern zwei Skalierungsmodelle.

2. **Kalibrierung (Soll/Ist).** Ein 10×10-mm-Schnitt wird real z. B. 10×18 mm.
   Der Nutzer soll die betroffene Achse kalibrieren können: „Soll 10, Ist 18"
   → die Schrittweite (Steps) der Achse im Controller anpassen. Das gilt nicht
   nur für Rotary, sondern für jede Achse (X/Y/Z/U).

3. **Architektur-Frage (der eigentliche Kern):** Rotary als Konzept ist **nicht
   treibergebunden** — ein Rotary funktioniert an Ruida, GRBL/grblHAL und
   FluidNC. Die **Kalibrierung** dagegen schreibt treiberspezifische Register/
   Settings. Wie wird das getrennt, ohne die Invariante „Fachlogik gehört in den
   Core, nicht in Treiber oder Native" (CLAUDE.md, ADR 0001) zu verletzen?

### Recherche: Kalibrierung ist bei allen Treibern dieselbe Fachlogik

Alle drei relevanten Firmwares können Achsen-Schrittkalibrierung. Der
Speicherort unterscheidet sich — und, **entscheidend**, auch die gespeicherte
Größe:

| Firmware        | Steps-Einstellung                     | Größe | Rotary       |
|-----------------|---------------------------------------|-------|--------------|
| Ruida           | `*_step_length` (`0x0021/31/41/51`)   | **Weg pro Schritt** (µm) | U bzw. Y-Rotary |
| GRBL / grblHAL  | `$100/$101/$102/$103`                 | **Schritte pro mm** | A (grblHAL)  |
| FluidNC         | `steps_per_mm` je Achse (config)      | **Schritte pro mm** | A (steps_per_degree) |

Die beiden Größen sind Kehrwerte voneinander, **die Kalibrierformel ist
deshalb nicht dieselbe**:

- **Weg pro Schritt** (Ruida): die gefahrene Strecke ist *umgekehrt*
  proportional zum Wert — der Controller fährt `Strecke / Schrittlänge`
  Schritte. Fährt die Achse zu weit, war die Schrittlänge zu klein und muss
  **steigen**: **`neu = alt × (Ist / Soll)`**.
- **Schritte pro mm** (GRBL/FluidNC): die Strecke ist *proportional* zum Wert.
  Fährt die Achse zu weit, muss der Wert **sinken**:
  **`neu = alt × (Soll / Ist)`**.

An Hardware gegengeprüft (Ruida, U-Achse): 10,6667 µm, Soll 10 mm, gemessen
25 mm → 26,6667 µm. Ein Durchgang trifft exakt; die Rechnung ist nicht bloß
konvergent, sondern genau, solange die Messung stimmt.

> **Frühere Fassung dieses ADR nannte `neu = alt × (Soll / Ist)` als für alle
> Firmwares identisch.** Das ist für Ruida falsch herum und führte in der ersten
> Umsetzung dazu, dass eine zu weit fahrende Achse nach dem Kalibrieren noch
> weiter fuhr. Wer einen neuen Treiber anbindet, muss zuerst klären, welche der
> beiden Größen dessen Register führt.

Die Rechnung ist geräteneutrale Fachlogik und gehört in den Core; welche der
beiden Formeln gilt, entscheidet die Einheit des Zielregisters.

## Entscheidung

**(A) Ein treiberneutrales Rotary-Fachmodell im Core mit den Bauarten Chuck und
Roller. (B) Die Achsenkalibrierung wird gespalten: die Soll/Ist→neuer-Wert-
Rechnung liegt im Core (treiberneutral, testbar), das Lesen/Schreiben des
konkreten Achsen-Steps-Registers im Treiber. (C) Der Treiber meldet über eine
Capability, ob er Steps-Kalibrierung überhaupt kann.**

### (A) Rotary-Fachmodell im Core

```
// studio-core, geräteneutral
pub enum RotaryKind {
    Chuck  { object_diameter_mm: f64 },   // Objekt dreht direkt
    Roller { roller_diameter_mm: f64 },   // Walze treibt; Objekt-Ø irrelevant
}

pub struct Rotary {
    pub kind: RotaryKind,
    pub steps_per_rev: f64,   // Motor-/Getriebeschritte pro Umdrehung
}

impl Rotary {
    /// Abwickel-Umfang in mm pro Umdrehung des treibenden Elements.
    pub fn circumference_mm(&self) -> f64 { … }   // Chuck: π·Objekt-Ø, Roller: π·Walzen-Ø
    /// Schritte pro mm Abwicklung (für die Achs-/Y-Skalierung).
    pub fn steps_per_mm(&self) -> f64 { … }
}
```

Der Core kennt **kein** Register und **kein** Gerät — nur die Physik. Jeder
Treiber bildet das auf seine Ausgabe ab:

- **Ruida U-Rotary**: setzt die passenden Rotary-/Step-Register.
- **Ruida Y-Rotary**: der Controller skaliert Y selbst aus seinen Registern
  (ADR 0021 §D) — Studio schreibt die Werte, rechnet aber nicht die Bewegung.
- **GRBL/FluidNC** (später): A-Achse `steps_per_mm`/`$103`.

Damit ist Rotary **nicht** an Ruida gebunden — genau die Anforderung.

### (B) Gespaltene Kalibrierung

```
// studio-core: reine Rechnung, testbar, kein Gerät
// Für Register, die den WEG PRO SCHRITT führen (Ruida *_step_length, µm).
pub fn calibrated_step_length(
    current_step_length: f64,
    target_mm: f64,
    measured_mm: f64,
) -> Result<f64, CalibrationError> {
    // Strecke ist umgekehrt proportional zur Schrittlänge:
    // zu weit gefahren → Schrittlänge muss steigen.
    current_step_length * (measured_mm / target_mm)   // + Schutz gegen ist≈0
}
```

Ein Treiber, dessen Register **Schritte pro mm** führt (GRBL `$10x`,
FluidNC `steps_per_mm`), braucht die gespiegelte Rechnung
`alt × (Soll / Ist)`. Beide gehören in den Core; die Einheit des
Zielregisters entscheidet, welche gilt.

Fehlerhafte Eingaben liefern bewusst ein `Result` statt eines Ersatzwerts:
ein stillschweigend „reparierter" Steps-Wert bewegt echte Hardware.

Der Fluss:

1. Native liest Soll und Ist vom Nutzer (z. B. „Soll 10, gemessen 25") und die
   Achse.
2. Application liest über den Treiber den **aktuellen** Wert der Achse.
3. **Core** rechnet den neuen Wert (`calibrated_step_length`).
4. Der **Treiber** schreibt den neuen Wert in sein Achsen-Register
   (Ruida: `write_machine_settings`, das existiert bereits; GRBL: `$10x`;
   FluidNC: config) und liest zur Kontrolle zurück.

So kennt der Core keine Registeradresse, und kein Treiber trägt die
Fachformel doppelt.

Weil der Ruida jedes Register einzeln beantwortet, dauert ein vollständiger
Lese-/Schreibgang mehrere Sekunden. Der gesamte Ablauf läuft deshalb auf dem
Geräte-Worker (wie `read_live_async`), nicht im UI-Thread.

### (C) Capability statt Annahme

```
pub struct DriverCapabilities {
    …
    pub axis_step_calibration: bool,   // kann der Treiber Steps kalibrieren?
}
```

Treiber ohne Steps-Kalibrierung (z. B. ein einfacher GRBL ohne
Schreibzugriff auf `$$`) melden `false`; die UI blendet die Kalibrierfunktion
dann aus, statt sie ins Leere laufen zu lassen. Ruida meldet `true`
(`write_machine_settings` + die vorhandenen `*_step_length`-Register).

### (D) Verortung in der UI: Laser-Verwaltung, pro Laser

Die Achs-Steps-Kalibrierung ist eine **Geräteeigenschaft** und gehört damit in
die **Laser-Verwaltung** (pro Laser), nicht in globale Settings. Die Verwaltung
hat bereits ein Tab-Layout (Grunddaten, Kalibrierung, Controller, Nullpunkte).

**Namensklärung — es gibt zwei verschiedene „Kalibrierungen":**

- Das **bestehende** Tab „Kalibrierung" ist die **Scan-Offset-/Reversal-**
  Korrektur (geschwindigkeitsabhängiger Versatz beim bidirektionalen Scannen,
  `scan_offset`). Das bleibt, wird aber zur Unterscheidung präziser benannt
  (z. B. „Scan-Offset").
- Neu kommt die **Achs-Steps-Kalibrierung** (Soll/Ist → Schrittweite) als
  eigener Bereich mit je einer Zeile pro Achse:

  ```
  Achskalibrierung
    X   [Soll mm] [Ist mm]  [Kalibrieren]
    Y   [Soll mm] [Ist mm]  [Kalibrieren]
    Z   [Soll mm] [Ist mm]  [Kalibrieren]   (nur wenn has_z_axis)
    U   [Soll mm] [Ist mm]  [Kalibrieren]   (nur wenn has_u_axis)
  ```

  Pro Achse: der Nutzer gibt Soll (gewünschte Strecke) und Ist (gemessene
  Strecke) ein, „Kalibrieren" liest den aktuellen Steps-Wert, rechnet (Core)
  und schreibt zurück (Treiber). Eine Achszeile erscheint nur, wenn die Achse
  existiert (Capabilities aus ADR 0021); der ganze Bereich nur, wenn
  `axis_step_calibration` — sonst ausgeblendet.

Ob das ein eigenes Tab „Achskalibrierung" neben „Scan-Offset" wird oder beide
Kalibrierungen ein gemeinsames Tab mit zwei Abschnitten teilen, ist eine
Feinheit der Umsetzung; fachlich sind es zwei getrennte Dinge.

## Konsequenzen

**Positiv**

- Rotary-Physik und Kalibrier-Rechnung sind **einmal** im Core, testbar, für
  alle Treiber gleich — keine Vermischung, keine Duplikation.
- Chuck und Roller sind sauber unterschiedliche Modelle statt eines
  fehleranfälligen „Durchmesser bedeutet mal dies, mal das".
- Neue Treiber (GRBL/FluidNC) bekommen Rotary + Kalibrierung, indem sie nur das
  Core-Modell auf ihre Register abbilden — ohne Fachlogik neu zu schreiben.
- Kalibrierung ist nicht rotary-spezifisch: dieselbe Funktion kalibriert X/Y/Z.

**Aufwand / Risiko**

- `studio-core` bekommt ein Rotary-Modul (Modell + `circumference_mm`/
  `steps_per_mm` + `calibrated_step_length`) mit Tests.
- `DriverCapabilities` und `LaserProfile` wachsen (Rotary-Parameter, gewählte
  Bauart) — rückwärtskompatibel über `serde(default)`.
- Die **Roller-Formel** ist bewusst auf „nur Walzendurchmesser" festgelegt; das
  ist an einer echten Kalibrierung zu bestätigen. Falls sich ein Aufbau anders
  verhält, bleibt `RotaryKind` erweiterbar.
- Der Ruida-Y-Rotary-Fall schreibt Werte, die der Controller selbst anwendet;
  Studio muss dort seine eigene Rechnung und die Controller-Register konsistent
  halten (Detail des Gravur-ADR).

## Nicht Teil dieser Entscheidung

- Die konkrete Rotary-**Gravur** (wann/wie ein Job als Rotary-Job kompiliert
  wird) — eigenes ADR, baut auf diesem Fachmodell auf.
- Die exakten GRBL/FluidNC-Abbildungen (dieses Projekt hat aktuell nur den
  Ruida-Treiber produktiv; das Modell ist so gebaut, dass sie später andocken).
- Die U/Z-Enable-Bit-Dekodierung (offen aus ADR 0021).
