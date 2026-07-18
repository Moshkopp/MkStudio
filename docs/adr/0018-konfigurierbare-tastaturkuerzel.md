# ADR 0018: Konfigurierbare Tastaturkürzel

- Status: angenommen
- Datum: 2026-07-18
- Betrifft: nativer Editor, globale UI-Einstellungen

## Kontext

Der native Eingabepfad lautet derzeit:

`winit::KeyboardInput -> canvas::input::map_key -> tools::resolve_shortcut -> Shortcut -> App::apply_shortcut`

`resolve_shortcut` enthält sowohl die feste Standardbelegung als auch wichtige
Sicherheitsregeln: Befehle lösen nur auf Key-Down aus, fokussierte Textfelder
und modale Dialoge blockieren Canvas-Aktionen, und das Loslassen der Leertaste
muss selbst bei nachträglich blockierter Eingabe durchkommen. Die Zuordnung ist
momentan nicht persistierbar und im Einstellungsdialog nicht sichtbar.

## Entscheidung

### 1. Getrennte Begriffe

Der Core erhält UI-freie, serialisierbare Typen:

- `ShortcutAction`: stabile semantische Aktion;
- `ShortcutKey`: logische Taste, unabhängig von winit und egui;
- `ShortcutChord`: Taste plus `ctrl`, `shift` und `alt`;
- `ShortcutTrigger`: Tastenkombination oder unterstützte Maustaste;
- `ShortcutBindings`: Zuordnung einer Aktion zu null, einer oder mehreren
  Eingabetriggern.

`ShortcutKey` deckt mindestens Buchstaben, Ziffern, `F1` bis `F12`, Pfeile,
`Delete`, `Backspace`, `Home`, `End`, `PageUp` und `PageDown` ab. Die
Standardbelegung dieses ADR verwendet davon ausdrücklich `B`, `P`, `T` sowie
`F1` bis `F5`.

Die gespeicherte Aktion ist nicht das native Laufzeit-Enum `Shortcut`.
`ShortcutAction` bleibt stabil über UI-Umbauten; der native Adapter übersetzt
sie erst nach erfolgreicher Auflösung in `Shortcut` beziehungsweise `Tool`.

Alle serialisierten Varianten verwenden explizite `snake_case`-Namen. Die
Reihenfolge einer Liste ist keine Identität.

### 2. Belegbare Aktionen

Belegbar werden:

- Projekt speichern und neue Version speichern;
- Rückgängig und Wiederholen;
- Alles auswählen, Löschen, Gruppieren, Gruppierung lösen und Ansicht
  einpassen;
- alle vorhandenen Zeichen-/Editorwerkzeuge: Auswahl, Rechteck, Ellipse,
  Polygon, Linie, Polylinie, Spline, Bézier, Messen, Knoten, Trimmen und
  Haltesteg;
- den Textdialog öffnen;
- die Hauptansichten Projekt, Design, Laser und Laser-Vorschau öffnen;
- die globale Asset-Bibliothek öffnen.

Ansichtswechsel und Asset-Bibliothek sind globale UI-Aktionen, keine
Canvas-Werkzeuge. `F5` verwendet denselben Anwendungsweg wie der vorhandene
„Assets“-Button: Projektansicht öffnen und dort direkt den Asset-Katalog
anzeigen.

Eine Aktion darf mehrere Kombinationen besitzen. Das erhält insbesondere die
beiden Redo-Standards `Ctrl+Shift+Z` und `Ctrl+Y`. Eine Aktion darf auch
vollständig unbelegt sein.

### 3. Statische Sicherheitskombinationen

Folgende Eingaben bleiben außerhalb der konfigurierbaren Tabelle:

- `Space` gedrückt/losgelassen: gehaltener Pan-Modifier;
- `Escape`: laufende Geste oder Dialog abbrechen;
- `Enter`: laufenden Punktpfad abschließen.

Sie sind zustandsabhängige Eingabesteuerung und keine gewöhnlichen Befehle.
Insbesondere muss `Space`-Release unabhängig vom aktuellen Fokus verarbeitet
werden, damit der Pan-Zustand nie hängen bleibt.

Der Recorder lehnt außerdem reine Modifier, Kombinationen mit Super/Meta sowie
vom Fenstersystem abgefangene Kombinationen wie `Alt+F4` ab. `Ctrl+F` bleibt
für eine spätere Suche reserviert. Diese Entscheidungen werden im Dialog
verständlich begründet, statt die Eingabe still zu ignorieren.

Die linke Maustaste bleibt die primäre Canvas-Bedienung und die mittlere
Maustaste bleibt fest für Pan reserviert. Die rechte Maustaste darf als
Action-Trigger belegt werden. Kontextmenüs dürfen sie künftig nur verwenden,
wenn die Benutzerbelegung dies nicht beansprucht oder wenn ein klar
abgegrenzter UI-Bereich außerhalb des Canvas getroffen wurde.

### 4. Standardbelegung

Die Migration übernimmt die bisherigen allgemeinen Befehle und ersetzt die
alte Werkzeugbelegung `P = Polygon` durch die hier festgelegte erweiterte
Standardbelegung:

| Aktion | Standard |
|---|---|
| Speichern | `Ctrl+S` |
| Neue Version | `Ctrl+Shift+S` |
| Rückgängig | `Ctrl+Z` |
| Wiederholen | `Ctrl+Shift+Z`, `Ctrl+Y` |
| Alles auswählen | `Ctrl+A` |
| Ansicht einpassen | `F` |
| Löschen | `Delete` |
| Gruppieren | `G` |
| Gruppierung lösen | `Ctrl+G` |
| Auswahlwerkzeug | `V`, rechte Maustaste |
| Rechteck | `R` |
| Ellipse | `E` |
| Polylinie | `P` |
| Polygon | `Ctrl+P` |
| Bézier | `B` |
| Text | `T` |
| Trimmen | `Ctrl+T` |
| Projektansicht | `F1` |
| Designansicht | `F2` |
| Laseransicht | `F3` |
| Laser-Vorschau | `F4` |
| Asset-Bibliothek | `F5` |

Alle weiteren Werkzeuge starten unbelegt. Die Defaults liegen an genau einer
Stelle im Core und werden von Migration, Einzel-Reset, Gesamt-Reset und Tests
gemeinsam verwendet.

### 5. Persistenz und Migration

`UiSettings` erhält ein mit `#[serde(default)]` versehenes Feld
`shortcut_bindings`; die Formatversion steigt bei der Implementierung von 3
auf 4. Fehlt das Feld, werden die vollständigen Standardbindungen eingesetzt.
Benutzerdefinierte Bindungen werden beim Laden normalisiert:

- unbekannte zukünftige Aktionen werden von einer alten Version nicht
  erfunden;
- identische Chords innerhalb derselben Aktion werden dedupliziert;
- ungültige oder reservierte Chords werden entfernt;
- ein bereits gespeicherter Konflikt zwischen zwei Aktionen wird nicht still
  nach Reihenfolge aufgelöst, sondern als Validierungsfehler gemeldet. Neue
  Konflikte löst der Dialog vor dem Speichern über die bestätigte
  Umbelegung auf.

Die Charon-Sicherung transportiert das Feld automatisch als Bestandteil der
globalen `UiSettings`. Shortcuts sind arbeitsplatzbezogen, nicht projekt- oder
laserprofilbezogen.

### 6. Auflösung und Fokusregeln

Der Laufzeitpfad wird zu:

`KeyboardInput -> ShortcutChord -> statische Sicherheitsregel -> Bindings-Lookup -> ShortcutAction -> Shortcut -> App`

Die bestehende Sperre bleibt vor dem konfigurierbaren Lookup erhalten:

- gewöhnliche Befehle nur auf Key-Down;
- keine Befehle bei fokussiertem Textfeld oder modalem Dialog;
- `Space`-Release als einzige notwendige Ausnahme;
- die bestehende Read-only-Regel des Laser-Tabs bleibt nach der Auflösung
  weiterhin wirksam.

Damit kann eine neue Belegung niemals die Fokus-/Modal-Sicherheitsgrenze
umgehen.

### 7. Einstellungsdialog

Der Einstellungen-Dialog erhält links den eigenen Menüpunkt „Tastenkürzel“.
Rechts erscheint eine scrollbare Tabelle mit den Spalten:

| Aktion | Belegung | Zurücksetzen |
|---|---|---|
| Speichern | `Ctrl+S` | ↶ |
| Polylinie | `P` | ↶ |

Die Aktionszeilen sind nach „Allgemein“, „Bearbeiten“, „Werkzeuge“ und
„Ansichten“ gruppiert. Die Tabelle kann später bei Bedarf um eine Suche
ergänzt werden; für die erste Umsetzung ist sie vollständig scrollbar und
zeigt alle Aktionen ohne weitere Unterdialoge.

Das Belegungsfeld sieht wie ein fokussierbares Eingabefeld aus, ist aber kein
freies Textfeld. Ein Linksklick in dieses Feld startet den Recorder. Der
auslösende Linksklick wird ausdrücklich nicht als neue Belegung erfasst.
Während der Aufnahme erhält das Feld eine Akzentumrandung und zeigt
„Tastenkombination drücken …“. Gehaltene Modifier werden unmittelbar sichtbar,
zum Beispiel „Ctrl + …“.

Hat eine Aktion mehrere Belegungen, zeigt dasselbe Feld mehrere einzelne Chips
(beispielsweise `Ctrl+Shift+Z` und `Ctrl+Y`) sowie ein kleines `+`. Klick auf
einen Chip zeichnet dessen Ersatz auf, `+` fügt eine weitere Belegung hinzu.
Jeder Chip kann einzeln entfernt werden; mindestens eine Belegung ist nicht
vorgeschrieben.

Der nächste zulässige Key-Down-Trigger oder die nächste unterstützte Maustaste
wird als Entwurf übernommen. Erst das erste Nicht-Modifier-Ereignis beendet
die Aufnahme. `Escape` bricht ausschließlich die laufende Aufnahme ab und
ändert die bestehende Belegung nicht. Ein Klick außerhalb des Feldes bewirkt
dasselbe. Es gibt kein freies Texteingabefeld und keine manuelle String-Syntax.

Die Standardbelegung „rechte Maustaste = Auswahlwerkzeug“ ist eine temporäre
Werkzeugübersteuerung: Rechtsklick und Rechtsziehen verwenden Hit-Test,
Auswahl, Marquee sowie Move/Resize/Rotate wie das Auswahlwerkzeug. Nach dem
Loslassen bleibt das zuvor aktive Zeichenwerkzeug erhalten. Ein einfacher
Rechtsklick schaltet das Werkzeug also nicht dauerhaft auf Auswahl um.

Vor Übernahme eines Triggers prüft der Entwurf:

- reservierte Kombination;
- identische Kombination derselben Aktion;
- Doppelbelegung durch eine andere Aktion.

Bei einer Doppelbelegung öffnet sich eine Bestätigungsabfrage, die Trigger und
beide Aktionsnamen nennt, zum Beispiel:

> `Ctrl+G` ist bereits „Gruppierung lösen“ zugewiesen. Für „Gruppieren“
> umbelegen?

Die Aktionen lauten bewusst:

- **Umbelegen** entfernt genau diesen Trigger bei der bisherigen Aktion und
  fügt ihn der neuen Aktion atomar hinzu;
- **Abbrechen** verwirft die neue Aufnahme und lässt beide Aktionen
  unverändert.

Besitzt die bisherige Aktion weitere Trigger, bleiben sie erhalten. War es
ihre einzige Belegung, ist die Aktion anschließend unbelegt; das ist zulässig
und wird in der Tabelle als „Nicht belegt“ angezeigt. Eine Option, denselben
Trigger trotzdem doppelt aktiv zu lassen, gibt es nicht, weil die Auslösung
sonst von einer versteckten Prioritätsregel abhängen würde.

Jede Tabellenzeile erhält rechts eine kleine Aktion „Zurücksetzen“, die nur
diese Belegung auf ihren Standard setzt. Unter der Tabelle stehen die
Dialogaktionen:

- **Speichern** persistiert den konfliktfreien Entwurf und aktiviert ihn sofort;
- **Abbrechen** verwirft alle seit dem Öffnen des Dialogs vorgenommenen
  Shortcut-Änderungen;
- **Standards wiederherstellen** setzt nach einer Bestätigung die gesamte
  Tabelle im Entwurf zurück. Persistiert wird dieser Reset erst mit
  „Speichern“.

Bei einem noch nicht bestätigten Konflikt oder reservierten Trigger bleibt
„Speichern“ deaktiviert. Reservierte Trigger werden direkt an der Tabellenzeile
erklärt; Konflikte werden durch „Umbelegen“ oder „Abbrechen“ vollständig
aufgelöst. Es gibt keine nur kurz sichtbare Toast-Meldung.

### 8. Verantwortungsgrenzen

- `luxifer-core`: persistierbare Typen, Defaults, Normalisierung, Validierung;
- `native/canvas/input`: winit-Taste in `ShortcutKey` übersetzen;
- `native/canvas/input`: rechte Maustaste als Action-Trigger und temporäre
  Auswahlgeste übersetzen;
- `native/tools`: Fokus-/Key-Flanken-Regeln und Aktion in Laufzeitbefehl
  übersetzen;
- `native/ui/dialogs/settings`: Recorder und Darstellung, keine eigene
  Shortcut-Wahrheit;
- `App`: führt den bereits typisierten Laufzeitbefehl aus.

## Abgelehnte Alternativen

### Freier String pro Shortcut

Abgelehnt, weil Layoutvarianten, Tippfehler, Modifier-Reihenfolge und
reservierte Tasten dann erst spät oder uneinheitlich validiert würden.

### Direktes Serialisieren von winit- oder egui-Tasten

Abgelehnt, weil externe UI-Typen kein stabiles Dateiformat darstellen und der
Core dadurch an eine Präsentationsbibliothek gekoppelt würde.

### Letzte Doppelbelegung gewinnt

Abgelehnt, weil dadurch Aktionen unbemerkt unerreichbar werden und das Ergebnis
von Listenreihenfolgen abhängt.

### Auch Space, Escape und Enter frei belegbar machen

Abgelehnt, weil diese Tasten laufende Gesten und gehaltene Zustände steuern.
Eine normale Action-Tabelle kann ihre Key-Up- und Modal-Semantik nicht sicher
abbilden.

## Test- und Abnahmematrix

Core:

- vollständige stabile Defaultbelegung;
- JSON-Roundtrip und Migration einer Version-3-Datei ohne Shortcut-Feld;
- mehrere Chords für eine Aktion;
- Deduplizierung innerhalb einer Aktion;
- Konflikt, reservierter Chord und ungültiger Chord;
- Einzel- und Gesamt-Reset.

Native Auflösung:

- benutzerdefinierter Chord löst die richtige Aktion aus;
- entfernte Standardbelegung löst nicht mehr aus;
- Redo-Aliase bleiben standardmäßig erhalten;
- Werkzeugbelegungen unterscheiden `P`/`Ctrl+P` und `T`/`Ctrl+T` korrekt;
- `G` gruppiert und `Ctrl+G` löst die Gruppierung;
- `F1` bis `F5` öffnen Projekt, Design, Laser, Laser-Vorschau und Assets;
- Rechtsklick und Rechtsziehen verwenden temporär Auswahl/Marquee/Transform,
  ohne das aktive Zeichenwerkzeug dauerhaft zu ändern;
- Key-Up löst keinen normalen Befehl aus;
- Fokus und modaler Dialog blockieren auch benutzerdefinierte Befehle;
- `Space`-Release funktioniert weiterhin bei blockierter Eingabe;
- Laser-Read-only-Grenze bleibt unverändert.

Dialog:

- Recorder statt Textfeld;
- Linksklick startet die Aufnahme, ohne selbst aufgezeichnet zu werden;
- sichtbarer Aufnahmezustand und live angezeigte Modifier;
- Konfliktdialog mit atomarem „Umbelegen“ und folgenlosem „Abbrechen“;
- beim Umbelegen wird nur der kollidierende Trigger der alten Aktion entfernt;
- verständliche Reserviert-Meldungen;
- Abbrechen verwirft den Entwurf;
- Einzel-Reset und bestätigtes „Standards wiederherstellen“;
- Speichern persistiert und wirkt ohne Neustart.

## Umsetzungsreihenfolge

1. Core-Typen, Defaults, Validierung und Settings-Migration.
2. Native Eingabeauflösung auf Bindings umstellen, statische Tasten erhalten.
3. Settings-Sektion und Recorder ergänzen.
4. Fokus-, Konflikt-, Persistenz- und Dialogtests vervollständigen.
5. Release-Gegencheck auf deutschem Tastaturlayout.

## Umsetzungsprotokoll

### Core-Modell und Settings-Migration

`luxifer-core` besitzt nun die UI-freien Typen `ShortcutAction`,
`ShortcutKey`, `ShortcutChord`, `ShortcutTrigger` und `ShortcutBindings`.
Defaults, Labels, Kategorien, Reserviert-Prüfung, Deduplizierung, Konfliktsuche,
Entfernen, Einzel-Reset und atomare Umbelegung liegen an dieser zentralen
Stelle. Die bestätigten Tastatur- und Rechtsmaus-Defaults sind vollständig
abgebildet; Redo behält beide Standardtrigger.

`UiSettings` Format 4 persistiert die Bindings arbeitsplatzbezogen. Version-3-
Dateien ohne Feld migrieren über `#[serde(default)]` exakt auf die neuen
Defaults. Laden normalisiert die Tabelle und lehnt verbleibende Konflikte ab.
252 Core-Tests einschließlich Defaults, Reserviert-Regeln, Umbelegung,
JSON-Roundtrip und Version-3-Migration sind grün.

### Native Action-Auflösung und temporäre Rechtsauswahl

Der native Tastaturpfad bildet nun alle unterstützten logischen winit-Tasten
einschließlich `F1` bis `F12` auf `ShortcutKey` ab und löst gewöhnliche Befehle
über die aktiven `UiSettings.shortcut_bindings` auf. `Space`, `Escape` und
`Enter` bleiben davor statisch mit ihrer bisherigen Key-Flanken- und
Fokussicherheit. Die Action-Übersetzung deckt Projektbefehle, Bearbeiten,
sämtliche Werkzeuge, Text, F1–F5 und Assets ab; Ansichten verwenden dieselben
App-Absichten wie die Topbar.

Ist die rechte Maustaste dem Auswahlwerkzeug zugewiesen, startet sie direkt die
bestehende Select-Geste und beendet sie beim Loslassen. Das aktive
Zeichenwerkzeug wird dabei nicht verändert. Wird die rechte Maustaste später
einer anderen Action zugewiesen, löst sie diese einmal auf Mouse-Down aus.
53 native Tests sichern unter anderem benutzerdefinierte Bindings,
Fokusblockade und die temporäre Rechtsauswahl.
