# ADR 0004: Asset-Store & Bild-Import/-Bearbeitung

## Status
Akzeptiert — 2026-07-08

## Kontext

Bisher entsteht Geometrie nur durch Zeichnen. Der Nutzer will Vorlagen
**importieren** und gravieren — zuerst **Bilder** (später SVG/DXF/PDF über
denselben Button). ADR 0003 hat dafür `asset_refs` und eine geplante zentrale
Asset-Bibliothek vorgesehen, den Store selbst aber vertagt. Da ein Bild das
**erste Asset** ist, entscheidet dieses ADR beides zusammen: den Store und den
Bild-Import darauf.

Zwei Erfahrungen aus ThorBurn (Referenz, docs/referenz/) prägen die Entscheidung:

- **Assets nie ins Projekt kopieren.** ThorBurn kopierte importierte Bilder pro
  Projekt — das war der Import-Fehler. Assets gehören in eine zentrale,
  projektübergreifende Ablage; Projekte referenzieren nur (ADR 0003 Invariante 2).
- **Quelldatei nie anfassen; Bearbeitung ist nicht-destruktiv.** ThorBurns
  Bildpipeline war brauchbar bei der Tonwert-LUT (Helligkeit/Kontrast/Gamma/
  Invert), aber schlecht beim Dithering und vermischte Vorschau mit Originaldaten.
  Wir trennen strikt: Die **Datei auf der Platte** (Dialog-Auswahl) wird nur
  gelesen, nie verändert. Das **Store-Asset** ist LuxiFers eigene Kopie; alle
  Regler sind Parameter, die erst bei Vorschau/Rastern angewandt werden — das
  Asset selbst bleibt unverändert.
- **ThorBurns Zähigkeit vermeiden.** Dort war „jedes Move/Speichern zäh", weil
  die Bildpipeline **an der Aktion hing** (Move rechnete neu, Speichern schrieb
  Pixel), und die **Canvas-Qualität schlecht**, weil das Bild zu Graustufe **und
  auf ~1024 px heruntergerechnet** wurde, damit es flüssig blieb. Beides schließen
  wir aus: Move/Speichern fassen keine Pixel an (§3a), und das Canvas zeigt das
  Bild in **voller Auflösung** (4K ist auf heutiger Hardware kein Problem).

**Nutzungsprofil (prägt die Prioritäten):** Der Nutzer graviert überwiegend
**S/W-Illustrationen und Ausmalbilder**, selten bis nie Fotos. Damit ist
**Schwellwert** der Hauptfall, nicht Fehlerdiffusion. Dithering wird deshalb
bewusst **vertagt** (siehe „Nicht Teil dieser Entscheidung").

## Entscheidung

### 1. Zentraler Asset-Store

```
<data_root>/
  Assets/
    <hash>.<ext>        Asset-Bytes (Graustufe, volle Auflösung), per Content-Hash
    <hash>.meta.json    Metadaten (Originalname, Format, Breite/Höhe px, import_at)
```

- **Zwei getrennte „Originale".** (1) Die **Quelldatei auf der Platte**, die der
  Nutzer im Dialog wählt — nur gelesen, nie verändert. (2) Das **Store-Asset** =
  LuxiFers Kopie, die beim Import entsteht. Wenn dieses ADR „Original" sagt, meint
  es das Store-Asset.
- **Beim Import zu Graustufe konvertieren, Farbe verwerfen.** Ist die Quelle
  farbig, wird sie in **voller Auflösung** entsättigt und **die Graustufe** als
  Asset abgelegt; die Farbe wird nicht mitgespeichert. War die Quelle bereits
  S/W/Graustufe, wird sie unverändert übernommen. Begründung: Der Laser braucht
  ohnehin Graustufe (er kennt keine Farbe), und das Canvas zeigt Bilder farblos —
  die Graustufe ist also ein **echter Pipeline-Schritt**, keine Anzeige-Krücke.
  So wird **einmal** beim Import entsättigt statt bei jedem Öffnen jedes Projekts.
  Wird das Bild je farbig gebraucht, importiert man die Plattendatei neu (sie ist
  unberührt) — es geht nichts verloren.
- **Graustufen-Methode:** vorerst fix **Luminanz** (gamma-korrekt, sRGB-
  linearisiert — `image::to_luma8`, fotografisch korrekt statt naiver
  Rec.601-Gewichtung), kein Import-Dialog. Eine spätere Methodenwahl (Kanal/
  Gewichtung, für getönte Vorlagen) ist im Design vorgesehen, aber nicht jetzt.
- **Identität = Content-Hash** der Asset-Bytes (z. B. FNV/SHA-artig, ohne
  Fremd-Crate — analog `gen_id`). Gleiches Bild zweimal importiert → **ein**
  Asset.
- Der Store ist **projektübergreifend**. Projekte verweisen nur über die
  Asset-ID (= Hash) in `asset_refs` und im jeweiligen Shape.
- **Nie kopieren pro Projekt.** Ein Asset existiert genau einmal auf der Platte.
- Kern-API im Core (UI-frei, testbar): `store_asset(bytes, ext) -> AssetId`,
  `load_asset(id) -> Bytes`, `asset_meta(id) -> AssetMeta`. Aufräumen
  verwaister Assets (kein Projekt referenziert sie mehr) ist vorgemerkt, nicht
  jetzt.

### 2. Import-Button & Bild-Objekt

- **Ein Import-Button** für alle Formate (Design-Reiter). Dieses ADR implementiert
  nur **Bilder** (PNG/JPG/JPEG/BMP/WebP); SVG/DXF/PDF melden „noch nicht
  unterstützt". Die Dispatch-Struktur nach Endung wird angelegt.
- Beim Import:
  1. Quelldatei lesen; bei Farbe zu **Graustufe (volle Auflösung, Luminanz)**
     konvertieren, Farbe verwerfen (§1). Die Graustufen-Bytes in den Store legen
     → `AssetId`.
  2. Ein **Bild-Objekt** erscheint auf dem Canvas in **voller Auflösung** (kein
     Downscale), mit Bounding-Box, mittig oder an Cursor platziert, in mm skaliert
     (aus px + DPI der Quelle bzw. Standard).

- **Datenmodell:** neue Geometrie-Variante `Geo::Image`:

  ```rust
  Geo::Image {
      asset: AssetId,     // Verweis in den Store (Original)
      x: f64, y: f64,     // Position der Bounding-Box (mm)
      w: f64, h: f64,     // Größe (mm)
      // Bildverarbeitungs-Parameter — nicht-destruktiv (§3)
      params: ImageParams,
  }
  ```

  Das Bild-Objekt ist ein normaler `Shape` (rotier-/verschieb-/skalierbar wie die
  anderen). Hit-Test/Bounds nutzen die Bounding-Box.

- **Resize-Verhalten (gilt für ALLE Objekte, nicht nur Bilder):** Die vier
  **Ecken-Handles (NW/NE/SW/SE) halten immer das Seitenverhältnis**; die vier
  **Kanten-Handles (N/S/O/W) skalieren frei** in einer Achse (bewusstes
  Verzerren). Damit ist proportionales Skalieren der Default und Verzerren
  weiterhin möglich, ohne Modifier-Taste. Dies ändert das bisherige Ecken-
  Verhalten von Rect/Ellipse/Polyline (vorher frei) — bewusst vereinheitlicht.

### 3. Bild-Bearbeitung (Doppelklick öffnet Editor)

**Doppelklick** auf das Bild-Objekt öffnet ein Fenster mit **Live-Vorschau** und
Reglern. Alle Werte sind **nicht-destruktive Parameter** in `ImageParams`; das
Original im Store wird nie verändert.

```rust
struct ImageParams {
    mode: ImageMode,       // Grayscale | Threshold  (Dither später)
    threshold: u8,         // 0..255, nur bei Threshold
    brightness: i32,       // -100..+100
    contrast: i32,         // -100..+100
    gamma: f64,            // 0.1..3.0
    invert_editor: bool,   // Invertiert die Canvas-Darstellung
    invert_laser: bool,    // Invertiert nur die Laser-/Rastervorschau
}
```

- **Modus:** vorerst **Grayscale** und **Threshold** (Schwellwert). Threshold ist
  der Hauptfall (S/W-Illustrationen, Ausmalbilder). Tonwert-Regler
  (Helligkeit/Kontrast/Gamma) wirken vor der Schwelle — die LUT-Logik aus
  ThorBurn (`image_adjust.rs`) ist der brauchbare Teil und wird im aktuellen Stil
  neu implementiert (kein Code kopiert).
- **Zwei getrennte Invert-Schalter:**
  - `invert_editor` invertiert die **Canvas-Darstellung** (das Einzige, was die
    Canvas-Anzeige verändert).
  - `invert_laser` invertiert **nur die Laser-/Rastervorschau**, nicht das Canvas.
- **Qualität im Canvas bleibt unverändert** — abgesehen von `invert_editor`. Die
  Regler wirken auf Vorschau und späteres Rastern, nicht auf die dargestellte
  Bildqualität.

### 3a. Nicht-destruktive Pipeline & Caching (Performance)

Das Store-Asset bleibt unverändert — **ohne** dass bei jeder Aktion alles neu
gerechnet wird. Der Trick: Das unveränderliche Grau-Asset (volle Auflösung) ist
die stabile **Cache-Wurzel** einer gestuften Pipeline. Jede Stufe wird nur neu
berechnet, wenn sich ihre Eingabe wirklich ändert:

```
Grau-Asset (Store, volle Auflösung,     ── einmal geladen, nie neu berechnet
            unveränderlich)                 (schon beim Import entsättigt)
      │  im RAM, solange Projekt offen
      ▼
Canvas-Anzeige                          ── zeigt das Grau-Asset in voller
      │                                      Auflösung direkt (kein Downscale)
      │  nur wenn ein Editor-Regler sich ändert
      ▼
Vorschau-Bitmap (Schwelle/Tonwert/      ── neu NUR bei Regler-Bewegung, per LUT
      │   invert_editor)                    (256er-Tabelle, ein Durchlauf)
      │  nur beim Rastern (Job)
      ▼
Laser-Raster                            ── ganz am Ende, einmalig
```

Konkrete Regeln, welche Aktion was auslöst:

- **Move / Resize / Rotate** berühren die **Pixel nicht**. Es ändern sich nur
  `x/y/w/h/rotation`; das Frontend skaliert das bereits geladene Bitmap beim
  Zeichnen (Canvas/GPU, praktisch gratis). **Keine Neuberechnung der Bilddaten.**
- **Speichern** schreibt nur `asset`-ID + `ImageParams` (winzig), **nie**
  Bilddaten. (ThorBurns zähes Speichern kam vom Mitschreiben der Pixel.)
- **Editor-Regler** (Schwelle/Helligkeit/Kontrast/Gamma/Invert) berechnen die
  **Vorschau** neu — aber nur die Vorschau, nur bei Änderung, über eine schnelle
  256-Werte-LUT (ein Durchlauf über das Grau-Asset, Millisekunden). Nur solange
  der Editor offen ist.
- **Grau-Asset** wird im Normalbetrieb **nie** neu berechnet — es ist schon beim
  Import entsättigt und liegt im RAM, solange das Projekt offen ist. Beim Öffnen
  eines Projekts wird es einmal aus dem Store geladen (kein Entsättigen mehr).
- **Persistenz:** Gespeichert werden nur `asset`-ID + `ImageParams`. Vorschau/
  Laser-Raster sind reiner Cache und entstehen on demand.

Kurz: Volle Auflösung im Canvas kostet nichts (moderne Hardware, ein Bitmap ohne
Downscale), und teuer wird nur, was sich tatsächlich ändert — nicht Move, nicht
Speichern.

### 4. Bild erzeugt einen eigenen Image-Layer

- Ein importiertes Bild erzeugt **automatisch einen neuen Layer** mit
  **`LayerMode::Image`** (neuer Modus neben Cut/Fill/Raster). Das Layer-Panel
  zeigt statt „Fill"/„Cut" den Typ **„Image"**.
- **Farbe des Image-Layers:** deterministisch aus einem **reservierten Bereich
  außerhalb der `SWATCH_COLORS`** vergeben — garantiert **kollisionsfrei** zum
  Farbkatalog und zwischen mehreren Bildern. So bekommt **jedes Bild einen
  eigenen Layer** mit eigener, eindeutiger Kennfarbe. (Nicht „zufällig", damit
  reproduzierbar und ohne Kollisionsrisiko.)
- **Layer-Parameter für Image** (teils schon im `Layer` vorhanden):
  - **Passes** (Wiederholungen) — vorhanden.
  - **DPI / Zeilenabstand** — vorhanden (`dpi`, `line_step_mm`).
  - **Max-/Min-Leistung** — vorhanden (`power_pct`, `min_power_pct`).
  - **Geschwindigkeit** — vorhanden (`speed_mm_s`).
  - **Bidirektional** — **neu**: `bidirectional: bool` (Scan hin und zurück).

### 5. Rastern im Laser (später)

Das Bild-Objekt wird **später** beim Job-Aufbau in Rasterzeilen übersetzt
(LayerMode::Image → Rasterpfad, DPI/Zeilenabstand, bidirektional, Leistung/
Geschwindigkeit, `invert_laser`, Schwellwert). Die Job-Kompilierung ist **nicht**
Teil dieses ADR — hier entstehen nur Datenmodell, Import, Bearbeitung und Layer.

## Invarianten

1. **Quelldatei unantastbar.** Die vom Nutzer gewählte Datei auf der Platte wird
   nur gelesen. Das Store-Asset (LuxiFers Kopie) wird nach dem Import ebenfalls
   nie verändert — alle Bearbeitung ist nicht-destruktiv über `ImageParams`.
2. **Asset ist Graustufe, volle Auflösung.** Farbe wird beim Import verworfen; das
   Store-Asset ist bereits grau (Laser braucht ohnehin Graustufe). Das Canvas
   zeigt es in voller Auflösung, farblos, ohne Downscale.
3. **Assets werden referenziert, nie ins Projekt kopiert** (ADR 0003 Invariante
   2). Identität = Content-Hash; gleiches Bild = ein Asset.
4. **Ein Bild = ein Image-Layer mit eigener, katalogfremder Farbe.** Keine
   Kollision mit `SWATCH_COLORS` oder zwischen Bildern.
5. **Nicht-destruktiv = gecacht, nicht neu gerechnet.** Move/Resize/Rotate und
   Speichern berühren keine Pixel; Vorschau/Laser-Raster sind Cache und entstehen
   nur bei tatsächlicher Eingabeänderung (§3a). Persistiert werden nur `asset`-ID
   + `ImageParams`.
6. **Fachlogik im Core.** Store, Hash, Graustufen-/Schwellwert-/Tonwert-Rechnung
   liegen UI-frei in `luxifer-core` und sind testbar (CLAUDE.md Regel 1). Das
   Frontend zeichnet nur die Vorschau.
7. **Aus ThorBurn wird kein Code kopiert** (CLAUDE.md Regel 6) — nur die Idee der
   Tonwert-LUT wird im aktuellen Stil neu gebaut.

## Konsequenzen

- Der Asset-Store (`Assets/`) entsteht jetzt; `asset_refs` (ADR 0003) wird erstmals
  befüllt. Charon kann Assets später per Hash einmalig ablegen und zusammen mit
  referenzierenden Projektversionen an andere Arbeitsplätze ausliefern.
- `Geo` bekommt die Variante `Image`; Serialisierung bleibt vorwärts-tolerant.
- `LayerMode` bekommt `Image`; `Layer` bekommt `bidirectional`.
- Projekt-Versionen (ADR 0003): Ein Bild-Shape referenziert nur die Asset-ID —
  Versions-Snapshots bleiben schlank, das Bild liegt einmal im Store.

## Nicht Teil dieser Entscheidung

- **Dithering** (Floyd-Steinberg u. a.) — bewusst vertagt; Grau + Schwellwert
  decken das Nutzungsprofil (S/W-Illustrationen, Ausmalbilder) ab. Später als
  eigener `ImageMode`-Eintrag ergänzbar.
- **Graustufen-Methodenwahl** (Kanal/Gewichtung statt fix Luminanz) — im Design
  vorgesehen, jetzt fix Luminanz.
- **SVG/DXF/PDF-Import** — nur der gemeinsame Button/Dispatch wird vorbereitet.
- **Job-/Raster-Kompilierung** (Bild → Laserzeilen) — eigener Job-Teil.
- **Schärfung (Unsharp Mask)** — vorgemerkt, nicht jetzt.
- **Aufräumen verwaister Assets** — vorgemerkt, nicht jetzt.
