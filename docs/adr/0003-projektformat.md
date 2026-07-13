# ADR 0003: Projektformat, Speichern/Laden & Asset-Bibliothek

## Status
Akzeptiert — 2026-07-07 · **überarbeitet 2026-07-08 (Versions-Modell)**

## Kontext

Zum Testen und Arbeiten muss man Projekte **speichern und wieder laden** können.
Das Core-Fundament steht (`luxifer/core/src/project.rs`: `ProjectFile`,
`save_to_dir`, `load`, `list_projects`), aber es fehlen Tauri-Commands und
Frontend. Zugleich soll das Format so gebaut sein, dass **Charon** versionierte
Projektstände zwischen Arbeitsplätzen **verteilen** kann (Invariante: lokales
Speichern funktioniert immer zuerst und ohne Charon; Charon editiert oder
merged keine Projektinhalte selbst).

Drei Erkenntnisse prägen die Entscheidung:

- **Identität über Umbenennen hinweg.** Der bisherige `ProjectFile` nutzt den
  Ordnernamen als Identität. Für Sync ist das fragil (Umbenennen = neues Projekt,
  zwei Rechner kollidieren). Es braucht eine stabile ID + Zeitstempel.
- **Assets gehören nicht ins Projekt.** Bilder/Fonts/DXF/SVG mehrfach pro Projekt
  zu kopieren war ThorBurns Import-Fehler. Sie gehören in eine **zentrale,
  projektübergreifende Bibliothek**; Projekte verlinken nur per Referenz.
- **Es gibt nur Versionen, keinen separaten „Arbeitsstand".** Die erste Fassung
  trennte einen mutierbaren Arbeitsstand (`projekt.luxi`) von eingefrorenen
  Versionen. Das führte zu **Thumbnail-Drift** und zur Frage „was zeigt die große
  Vorschau" (Workaround: neueste Version). Die Trennung wird aufgehoben: **Die
  aktuelle Version *ist* der Canvas.** Das entfernt die Drift-Quelle vollständig.

## Entscheidung

### 1. Versions-Modell: Es gibt nur Versionen

**Kernentscheidung.** Ein Projekt ist eine geordnete Liste von **Versionen**
(V1, V2, V3 …). Es gibt **keinen** separaten mutierbaren Arbeitsstand daneben —
die **aktuelle Version *ist* das, was im Canvas bearbeitet wird**. „Hauptversion"
ist kein auswählbarer Extra-Zustand, sondern schlicht die aktuelle Version.

Regeln (alle vier Speicher-/Ladeaktionen):

- **Neues Projekt** → bekommt automatisch **V1** (leer bzw. der erste Entwurf).
- **Strg+S** (normal) → **aktualisiert die aktuelle Version** (überschreibt sie
  in-place). Erzeugt **keine** neue Nummer. Schreibt auch das Thumbnail der
  aktuellen Version neu.
- **Shift+Strg+S** → erzeugt die **nächste Version** (V2, V3 …) als Kopie des
  jetzigen Canvas-Stands (inkl. aller ungespeicherten Änderungen). Diese neue
  Version wird sofort zur **aktuellen**; der Canvas arbeitet auf ihr weiter.
- **Alte Version laden → ändern → Strg+S** → der **erste** Strg+S nach dem Laden
  einer *älteren* Version **verzweigt** einmalig in eine **neue** Version (statt
  die geladene alte zu überschreiben). Ab dann ist diese neue Version die
  aktuelle; jeder weitere Strg+S aktualisiert sie normal in-place.
- **Version löschen** (im Browser, mit Rückfrage) → entfernt eine einzelne
  Version. Wird die aktuelle Version gelöscht, wird die vorherige zur aktuellen.

**Woraus die alte Thumbnail-Drift verschwindet:** Das große Vorschau-Thumbnail
ist immer das Thumbnail der **aktuellen Version** — und weil es nur Versionen
gibt, existiert kein zweiter, driftender Speicherpfad mehr.

### 1a. Datenmodell (`ProjectFile`)

`ProjectFile` bekommt (alle neuen Felder `#[serde(default)]`, damit alte Dateien
weiter laden):

- `id: String` — stabile ID, bei Erstellung erzeugt, unveränderlich über
  Umbenennen. Erzeugt durch eigene `gen_id()` (Zeit + Zufall), **kein Fremd-Crate**.
- `created_at`, `modified_at` — ISO-8601 (UTC), über `std::time`.
- `description: String`, `tags: Vec<String>` (`tags` existiert bereits).
- `asset_refs: Vec<String>` — Liste von Asset-IDs, **vorerst leer** (vorbereitet).
- `versions: Vec<VersionInfo>` — die geordnete Versionsliste (mind. V1).
- `current_version: String` — ID der aktuellen Version (= was im Canvas ist).

Die **Geometrie eines Projekts liegt nicht mehr in `projekt.luxi`**, sondern
pro Version in `versions/<version-id>.luxi` (`bed`, `layers`, `shapes`).
`projekt.luxi` hält nur noch **Metadaten + Versionsliste + Zeiger auf die
aktuelle Version**. Öffnen = `current_version` in den Canvas laden.

`VersionInfo { id, label, created_at, note }` — `label` ist die anzeigbare Nummer
(„V3"), `id` die stabile interne Kennung. Thumbnails liegen als **Datei** neben
dem Snapshot (nicht im JSON), damit `projekt.luxi` schlank bleibt.

### 2. Ordnerstruktur auf Platte

```
<data_root>/
  Projekte/
    <Name>/
      projekt.luxi        NUR Metadaten (id, Zeitstempel, Beschreibung, tags,
                          asset_refs [], versions [], current_version).
                          Enthält KEINE Geometrie.
      versions/
        <version-id>.luxi Geometrie der Version (bed, layers, shapes)
        <version-id>.png  Thumbnail dieser Version
  Assets/                 (später, mit Import) zentrale Bibliothek,
                          projektübergreifend, per ID/Content-Hash
```

Ein Projekt hat **immer mindestens V1**; `versions/` ist nie leer. Öffnen lädt
`current_version` in den Canvas. `asset_refs` verweist auf `Assets/`, kopiert nie.
Der Store selbst kommt mit dem Import (eigene ADR); hier nur das Format-Feld.

### 3. Speicher-Workflows (GUI)

- **Neues Projekt** — ausgelöst über **Strg+N** oder den **„Neu"-Button** im
  Projekt-Reiter. Leert die Zeichenfläche und setzt den Projektkontext zurück
  (namenlos, `dirty = false`). Bei ungesicherten Änderungen greift zuvor der
  Unsaved-Guard (siehe unten). Beim ersten Speichern entsteht **V1**.
- **Strg+S**: namenloses Projekt → Projekt-Reiter öffnet sich (Name/Beschreibung/
  Tags ausfüllen, speichern → legt **V1** an). Benanntes Projekt → **aktuelle
  Version still aktualisieren** (in-place, inkl. Thumbnail-Neuschrieb), Toast
  „Gespeichert ✓ · Shift+Strg+S legt eine neue Version an". Sonderfall: Der erste
  Strg+S nach dem Laden einer **älteren** Version verzweigt in eine neue Version
  (siehe §1).
- **Shift+Strg+S**: **nächste Version** (V2, V3 … als Kopie des jetzigen
  Canvas-Stands, mit eigenem Thumbnail); wird zur aktuellen Version.
- **Datenschutz**: Neu/Öffnen/Version-laden bei ungesicherten Änderungen
  (`AppState::dirty`) → Nachfrage „Verwerfen / Speichern / Abbrechen". Gilt **auch
  für ein namenloses Projekt** (verklickter „Neu"-Button darf keinen Entwurf
  verlieren). Ist das Projekt noch namenlos, heißt die Speichern-Option
  „Speichern unter…" und öffnet den Projekt-Reiter zum Benennen (statt still zu
  überschreiben).
- **Start**: App startet leer im Designer (wie „Neu", aber ohne Guard); Toast
  „Zuletzt: ‹Name›" (Öffnen/Dismiss), Anker `last_project_id` in den GUI-Settings
  (ADR 0002).

### 4. Projekt-Reiter als Browser (volle Body-Fläche)

Links Suchfeld + Liste (Name, Tags, „geändert"), rechts Detail-Panel: Thumbnail
(= aktuelle Version), erstellt/geändert, Tags, Beschreibung, Versionsliste (je
Thumbnail + **laden** + **löschen** mit Rückfrage), Assets-Bereich („keine"),
**Charon-Status** (ehrlich „offline — nicht verbunden", bis Charon existiert).
Aktionen oben: **Neu**, Speichern. Am gewählten Projekt: Laden, Umbenennen,
Löschen, Export. Beim Löschen der aktuellen Version wird die vorherige zur
aktuellen; die letzte verbleibende Version (V1) ist nicht löschbar.

### 5. Thumbnail im Frontend

Das Thumbnail wird im **Frontend** aus der vorhandenen Canvas-Zeichenlogik in ein
Offscreen-Canvas gerendert und als PNG an den Core gereicht. Reine Darstellung,
kein Wahrheits-Zustand → konform mit „Frontend zeichnet nur" (CLAUDE.md Regel 2).
Der Core speichert nur die gelieferten Bytes als `versions/<version-id>.png`.

Es gibt **genau ein Thumbnail pro Version**, geschrieben bei jedem Speichervorgang,
der diese Version berührt (Strg+S in-place, Shift+Strg+S beim Anlegen). Die große
Vorschau im Browser zeigt das Thumbnail der **aktuellen Version** (`current_version`).
Weil nur Versionen existieren und jedes Speichern das Thumbnail der betroffenen
Version mitschreibt, kann das Vorschaubild **nicht** von der Geometrie abdriften —
das war die zentrale Baustelle der ersten Fassung.

## Invarianten

1. **Es gibt nur Versionen; die aktuelle Version *ist* der Canvas.** Kein
   separater Arbeitsstand. `current_version` zeigt immer auf genau eine Version.
2. **Identität = `id`, nicht der Ordnername.** Umbenennen ändert nie die `id`.
3. **Assets werden referenziert, nie ins Projekt kopiert.** Der zentrale Store ist
   die einzige Ablage für Bilder/Fonts/DXF/SVG.
4. **Charon ist nie Voraussetzung.** Speichern/Laden funktioniert vollständig
   offline; der Charon-Status ist reine Anzeige.
5. **Format ist vorwärts-tolerant.** Neue Felder mit serde-`default`; alte Dateien
   laden ohne Migration.
6. Die **Fachlogik (Format, Versionen, Speichern) liegt im Core** und ist ohne UI
   testbar (CLAUDE.md Regel 1).

## Konsequenzen

- Charon kann Projektversionen anhand stabiler Projekt-, Versions- und
  Eltern-IDs verteilen und geteilte Assets nur einmal ablegen. Parallele
  Änderungen bleiben erkennbare Zweige; Übernehmen, Vergleich und Merge sind
  explizite Clientaktionen.
- Der `last_project_id`-Anker erweitert die GUI-Settings (ADR 0002).
- Thumbnails kosten je Version eine kleine PNG-Datei — bewusst, für die visuelle
  Versionsliste.

## Nicht Teil dieser Entscheidung

- **Import** (Bilder/Fonts/DXF/SVG) und der **eigentliche Asset-Store** — eigene ADR.
- **Charon-Netzwerkprotokoll** — Charon bleibt vorerst leer.
- **Auto-Save** der aktuellen Version (nur vorgemerkt, nicht jetzt).
- **Migration des bestehenden Formats** (aktuell: Geometrie in `projekt.luxi`,
  `versions` als bloße Historie) auf das neue Modell (Geometrie pro Version,
  `current_version`-Zeiger) — Umsetzungs-Detail, folgt beim Code, nicht hier.
