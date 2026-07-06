# CLAUDE.md — Arbeitsrichtlinien für LuxiFer (Rust / Tauri / Svelte)

Verbindlich. Regeln mit **MUSS** / **DARF NICHT** sind Architektur-Invarianten;
Abweichung nur mit ausdrücklicher Zustimmung des Nutzers. Antworten und
Code-Kommentare auf Deutsch.

## Projekt in einem Satz

LuxiFer ist eine offline-first Desktop-Anwendung zur Laser-Steuerung.
**Die GUI ist das Produkt.** Charon (Rust) ist ein optionaler Koordinations-
Server und niemals Voraussetzung für lokale Arbeit.

## Stack (Neustart 2026-07-06, weg von C#/Avalonia)

- **luxifer-core** (Rust): Datenmodell, Geometrie, Layer/Farbe, Undo, später
  Projektformat und Job-Kompilierung. **Einzige Quelle der Wahrheit.**
- **LuxiFer-GUI**: Tauri + **Svelte**. Das Frontend **zeichnet nur** und ruft
  Core-Logik über Tauri-Commands.
- **Charon** (Rust): optionaler Server, teilt sich den Core. Aktuell leer.

## Verzeichnisse

| Pfad | Inhalt |
|------|--------|
| `crates/luxifer-core/` | Rust-Core (UI-frei, testbar) |
| `charon/` | Charon-Server (Rust, noch leer) |
| `luxifer/` | Tauri-App (Rust-Backend + Svelte-Frontend) — kommt, sobald Node da ist |
| `docs/referenz/` | ThorBurn-Analyse + Funktions-Worksheet (Bauplan) |
| `nur zur Referenu/` | Altes ThorBurn-Projekt — **nur Referenz, gitignored** |

## Architektur-Invarianten

1. **Fachlogik gehört in `luxifer-core`** (Rust), nicht ins Svelte-Frontend.
   Geometrie, Hit-Test, Bounds, Skalierung, Layer/Farbe, Undo, Job sind im Core
   und dort testbar. Faustregel: Was ohne UI testbar sein sollte, gehört in den
   Core. **Keine Canvas-Fachlogik doppelt im Frontend** (das war ThorBurns
   Fehler).
2. Das **Frontend zeichnet nur** und leitet Eingaben als Tauri-Commands an den
   Core weiter; es hält keinen eigenen Wahrheits-Zustand.
3. **Farbe = Layer = Parametersatz, automatisch verwaltet.** Der Nutzer legt NIE
   manuell einen Layer an. Farbe klicken → `AppState::activate_color` (bei
   Auswahl Shape in Farb-Layer verschieben, sonst `pending_color` merken); leere
   Layer werden über `remove_empty_layers` automatisch entfernt. Siehe
   docs/referenz/01-thorburn-analyse.md §1.5.
4. **Undo ist Snapshot-basiert** (`push_undo` vor jeder mutierenden Aktion),
   nicht Command-basiert.
5. **Charon steuert niemals eine Maschine** und ist nie Voraussetzung für lokale
   Arbeit.

## Referenz (ThorBurn)

6. Aus `nur zur Referenu/` und den Referenz-Dokumenten wird **kein Code
   kopiert.** Nur analysieren, wie eine Funktion gebaut war, und im aktuellen
   Stil sauber neu implementieren. Der Bauplan (Reihenfolge M1–M7) steht in
   `docs/referenz/02-funktions-worksheet.md`.

## Build, Test, Format

```bash
# Rust (aus Repo-Wurzel)
cargo build
cargo test        # müssen grün sein; neue Core-Logik bekommt Tests
cargo clippy
cargo fmt

# Frontend (später, sobald Node installiert ist)
# npm install && npm run tauri dev
```

7. **Vor jedem Commit:** `cargo build` + `cargo test` grün, `cargo clippy` ohne
   Warnungen, `cargo fmt`. Neue Core-Fachlogik bekommt Tests.

## Commits

- Sprache: Deutsch. Betreff im Imperativ, knapp; Body erklärt das *Warum*.
- Ein Commit = eine logische Änderung.
- Nur committen/pushen, wenn der Nutzer es verlangt.
- Footer: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
