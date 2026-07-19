# ADR 0001: Treiber-Abstraktion (Job unabhängig vom Gerät)

## Status
Akzeptiert — 2026-07-06

## Kontext

LuxiFer soll **mehrere Laser-Steuerungen** bedienen: **Ruida**, **GRBL**,
**miniGRBL** und potenziell weitere. Diese sprechen völlig verschiedene
Sprachen (Ruida: verschlüsselte Binär-Pakete über UDP; GRBL/miniGRBL:
G-Code-Text über serielle Verbindung).

Ruida ist damit **nur einer von mehreren Treibern** und darf die Kernlogik
nicht prägen. In der ThorBurn-Referenz gab es zwar ein `LaserController`-Trait
(gut), aber die Job-**Kompilierung** war Ruida-spezifisch
(`hardware/job/ruida_compiler`) — das wollen wir vermeiden.

## Entscheidung

**Die Fachlogik erzeugt einen geräteunabhängigen `JobPlan`; die Treiber
übersetzen ihn in ihr jeweiliges Format.**

### 1. Geräteunabhängiger JobPlan (im Core)

Der Core wandelt Shapes + Layer in einen **`JobPlan`**: die auszuführenden
Bewegungen in **mm**, gruppiert nach Layer (mit dessen Parametern). Kern-
Bausteine:

- **Cut**: Kontur-Pfade (Folge von Punkten, „mit Laser fahren").
- **Fill**: Scanline-Segmente (später).
- **Raster**: Bild-Bitmap + Parameter (später).

Der `JobPlan` kennt **kein** Ruida, **kein** G-Code, keine Bytes, keine
Verschlüsselung. Er ist rein geometrisch/parametrisch und testbar.

### 2. `MachineDriver`-Trait

Ein Treiber implementiert:

- `compile(&JobPlan, &[Layer]) -> Vec<u8>` — übersetzt den Plan in das
  gerätespezifische Format (Ruida-Bytes bzw. G-Code als UTF-8-Bytes).
- Live-Steuerung: `jog`, `home`, `go_origin`, `frame`, `stop`, `status`,
  `send_job` — über den jeweiligen Transport (UDP/seriell).
- Optionale Funktionen werden durch `DriverCapabilities` gemeldet. Dazu
  gehören derzeit geräteneutral beschriebene Maschineneinstellungen. Die
  Application spricht auch dafür nur mit `MachineDriver`; Registeradressen und
  Protokolldetails verbleiben im konkreten Treiber.

### 3. Treiber sind eigene Crates

`luxifer/drivers/ruida`, `luxifer/drivers/grbl`, `luxifer/drivers/minigrbl` —
jedes ein eigenes Crate, das nur `luxifer-core` (JobPlan + Trait) kennt.
Vorteile: klar getrennt, unabhängig testbar, und es ist strukturell sichtbar,
dass Ruida nur ein Treiber ist.

## Invarianten

1. **`luxifer-core` DARF NICHT** gerätespezifischen Code enthalten (keine
   Ruida-Bytes, kein G-Code). Es definiert nur `JobPlan` und `MachineDriver`.
2. Der **`JobPlan` ist die einzige Schnittstelle** zwischen Fachlogik und
   Treibern. Neue Schnitt-Arten erweitern den JobPlan, nicht die Treiber.
3. Ein neuer Treiber = neues Crate, das den Trait implementiert. Kein
   bestehender Code muss dafür geändert werden.

## Konsequenzen

- Cut/Fill/Raster-Berechnung liegt **einmal** im Core, nicht pro Treiber.
- Die GUI wählt einen Treiber und ruft ausschließlich über den Trait — sie
  kennt die Geräte-Details nie (analog zur ThorBurn-Regel „GUI spricht nur über
  den Controller").
- Reihenfolge: erst `JobPlan` + Cut-Pfade im Core (dieser Schritt), dann die
  Treiber-Crates.

## Nicht Teil dieser Entscheidung

Die konkreten Treiber-Implementierungen, der Transport (UDP/seriell) und die
Fill-/Raster-Details im JobPlan (kommen als eigene Schritte).
