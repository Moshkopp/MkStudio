Gesamturteil
Das Projekt hat inzwischen eine erstaunlich breite Funktionsbasis, aber die letzten Erweiterungen haben eine kritische Architekturgrenze sichtbar gemacht: Eine Form besitzt mittlerweile mehrere Darstellungen gleichzeitig – etwa geo, Bézier-Metadaten, Rotation und Text-Metadaten. Die Transformationsfunktionen verändern diese Darstellungen nicht zentral und konsistent.
Das ist aktuell der größte Ausbau-Blocker. Weitere Werkzeuge wie Rotation, Objektinspektor, Pfadoperationen oder präzises Ausrichten würden die Inkonsistenzen verstärken.

Was davon bereits umgesetzt ist

Stand 11.07.2026 wurden die ersten kritischen Grundlagen bereits korrigiert:

- Zentrale Shape-Transformationen für Verschieben, Skalieren und Spiegeln sind eingeführt. Die Operationen verändern nicht mehr ausschließlich `geo`, sondern halten auch editierbare Bézier-Metadaten synchron.
- Bézier-Anker und Tangenten werden bei Verschieben, Skalieren, Spiegeln, Arrange und Nesting gemeinsam mit der sichtbaren Kontur transformiert. Regressionstests sichern dieses Verhalten ab.
- Arrange bildet anhand von `group_id` echte Einheiten. Gruppierte Konturen und Textgruppen werden beim Ausrichten und Verteilen nicht mehr auseinandergerissen.
- Horizontale und vertikale Verteilung arbeitet nach Objektmitten. Zusätzlich stehen gleichmäßige horizontale und vertikale Zwischenräume zur Verfügung.
- Eine rotationskorrekte Welt-Bounding-Box ist im Core umgesetzt. Auswahl, Arrange, Hit-Test und Transformanzeige verwenden damit bei gedrehten Shapes die passende Weltgrenze.
- Die kanonische Auswahl-Bounding-Box wird direkt im Core berechnet und als Teil der `Scene` ausgeliefert. `App.svelte`, Transform-Leiste und Canvas-Auswahlrahmen setzen die Gruppenbox nicht mehr unabhängig zusammen.
- Die Transform-Leiste enthält X/Y, Breite/Höhe, Seitenverhältnis-Sperre und einen 3×3-Anker. Position und Skalierung beziehen sich auf den gewählten Ankerpunkt.
- Nullbreite und Nullhöhe werden bei der Seitenverhältnis-Berechnung abgefangen; horizontale und vertikale Linien erzeugen dort kein `Infinity` oder `NaN` mehr.
- Editierbare Text-Metadaten besitzen eine feste Transformationsregel: proportionale Skalierung aktualisiert `size_mm`; nichtproportionale Skalierung und Spiegelung entfernen die nicht mehr reproduzierbaren Textparameter und lassen sichere normale Konturen zurück.
- Die beiden bestehenden Clippy-Warnungen im Pattern-Fill-Beispiel und im UI-Settings-Test sind behoben; das vollständige Clippy-Gate kann wieder als belastbare Prüfung für neue Änderungen dienen.
- Der Bézier-Segment-Hit-Test liegt im Rust-Core. Das Frontend übergibt nur Weltposition und zoomabhängige Toleranz; Shape, Segment und Kurvenparameter `t` werden zentral und rotationsbewusst bestimmt.
- Alle Tauri-Aufrufe laufen im Frontend durch eine gemeinsame Invoke-Grenze. Fehler werden als `EditorError` mit Code, Meldung, Command und optionalen Details normalisiert, zentral protokolliert und über einen gemeinsamen UI-Fehlerkanal angezeigt. App, Projektbrowser und Textdialog verwenden dieselbe lesbare Meldung.
- Die zuvor offenen Änderungen sind in zwei getrennten Commits gesichert:
  - `ba52247 Stabilisiere Transformationen und Anordnen`
  - `97823ea Berücksichtige Rotation in Weltgrenzen`
- Der geprüfte Stand umfasst 211 erfolgreiche Rust-Tests, einen fehlerfreien `svelte-check` und einen erfolgreichen Produktions-Build.

Als größerer Strukturpunkt bleibt insbesondere die Modulzerlegung offen.

Bereits vorhandene Probleme
1. Kritisch: Bézier-Knoten werden bei Transformationen nicht mitgeführt
Ein Bézier-Shape besitzt:
die abgeflachte Kontur in shape.geo
die editierbaren Anker und Tangenten in shape.bezier
Verschieben, Skalieren, Spiegeln und Arrange verändern momentan nur geo:
[interact.rs (line 94)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/core/src/interact.rs:94)
[arrange.rs (line 39)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/core/src/arrange.rs:39)
[arrange.rs (line 74)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/core/src/arrange.rs:74)
Der Node-Editor liest danach aber wieder die unveränderten Bézier-Metadaten:
[Canvas.svelte (line 534)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/frontend/src/lib/Canvas.svelte:534)
Konkrete Folge:
Bézier-Kurve zeichnen.
Kurve verschieben, skalieren, spiegeln oder ausrichten.
Knotenwerkzeug aktivieren.
Die Knoten erscheinen an der alten Position.
Beim ersten Bearbeiten kann die sichtbare Kurve zurückspringen oder neu aus den veralteten Knoten aufgebaut werden.
Das sollte vor weiteren Transformationsfunktionen behoben werden.
Empfehlung: Transformationen als Methoden auf Shape umsetzen, die geo, bezier, Rotation und weitere Metadaten gemeinsam aktualisieren.
2. Kritisch: Gruppierte Formen werden durch Arrange auseinandergerissen
Die Auswahl wird zwar auf vollständige Gruppen erweitert, Arrange behandelt danach aber jedes Shape einzeln.
Beim Ausrichten wird für jedes Gruppenmitglied ein eigenes Delta berechnet:
[arrange.rs (line 53)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/core/src/arrange.rs:53)
Beim Verteilen wird ebenfalls jedes Shape als eigenständiges Element behandelt:
[arrange.rs (line 102)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/core/src/arrange.rs:102)
Damit kann beispielsweise ein aus mehreren Konturen bestehender Textblock beim Ausrichten oder Verteilen auseinandergezogen werden.
ThorBurn behandelte eine gemeinsame group_id als eine Arrange-Einheit. LuxiFer sollte im Core ebenfalls zuerst Einheiten bilden:
gruppierte Shapes → eine gemeinsame Einheit
ungruppierte Shapes → jeweils eine Einheit
Erst danach sollten Bounding-Box und Bewegungsdelta berechnet werden.
3. Hoch: Rotation wird in Bounding-Boxen nicht berücksichtigt
Shape::bbox() liefert nur geo.bbox():
[model.rs (line 213)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/core/src/model.rs:213)
Die Eigenschaft rotation wird dabei ignoriert. Der Hit-Test dreht zwar den Prüfpunkt zurück, aber Auswahlbox, Marquee, Arrange, Größenanzeige und Transform-Anker arbeiten weiterhin mit der ungedrehten Box.
Folgen bei gedrehten Objekten:
falsche Selektionsbox
falsche X/Y/B/H-Werte
falsche Ausrichtung
unpassende Skaliergriffe
eventuell fehlerhafte Job- oder Preview-Grenzen
Bevor Rotation in die neue Transform-Leiste eingebaut wird, braucht Shape eine kanonische Weltkontur oder mindestens eine rotationsbereinigte Welt-Bounding-Box.
4. Hoch: Die neue Transform-Leiste skaliert über eine Frontend-Bounding-Box
Die Auswahlbox wird in App.svelte erneut aus den übertragenen Shapes berechnet:
[App.svelte (line 230)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/frontend/src/App.svelte:230)
Auch Canvas.svelte besitzt eine eigene Bounding-Box-Berechnung:
[Canvas.svelte (line 270)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/frontend/src/lib/Canvas.svelte:270)
Gleichzeitig besitzt der Core bereits selection_bbox():
[interact.rs (line 80)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/core/src/interact.rs:80)
Damit existieren aktuell mindestens drei Berechnungswege. Sie unterscheiden sich spätestens bei Rotation, Bézier-Metadaten und zukünftigen Formtypen.
Das verletzt die gewünschte Grenze „Core ist die einzige Wahrheit“.
Empfehlung: Die Scene sollte die kanonische Auswahlbox direkt mitliefern oder ein read-only Core-Command dafür anbieten.
5. Hoch: Text-Metadaten passen nach Skalierung nicht mehr zur sichtbaren Form
Text wird als Konturgruppe gespeichert, trägt aber weiterhin ursprüngliche Angaben wie Font und size_mm.
Wenn ein Textblock geometrisch skaliert wird, ändern sich nur die Konturen. Die Text-Metadaten bleiben unverändert. Beim späteren Bearbeiten und Neugenerieren kann der Text daher auf seine ursprüngliche Größe zurückfallen.
Für editierbaren Text braucht es eine klare Entscheidung:
Skalierung in TextMeta übernehmen, oder
eine explizite Objekttransformation speichern, oder
Text beim ersten freien Pfadeingriff dauerhaft in normale Konturen umwandeln.
Ohne diese Grenze werden Textskalierung, Rotation und erneutes Editieren nicht stabil zusammenspielen.
6. Mittel: Bézier-Hit-Test und Kurvenberechnung liegen teilweise im Frontend
Das Frontend enthält eigene kubische Bézier-Auswertung und Segmentabtastung:
[Canvas.svelte (line 603)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/frontend/src/lib/Canvas.svelte:603)
[Canvas.svelte (line 614)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/frontend/src/lib/Canvas.svelte:614)
Das Teilen selbst erfolgt korrekt im Core, aber welches Segment und welcher Parameter t gewählt werden, entscheidet das Frontend über 32 Stichproben.
Probleme:
Trefferpräzision hängt vom Zoom und Sampling ab.
Frontend und Core können verschiedene Kurvenannahmen entwickeln.
Touch- oder Stiftinteraktion benötigt wieder eigene Logik.
Eine spätere GPU-Canvas-Umstellung müsste Fachlogik übernehmen.
Die Vorschau darf lokal bleiben. Der tatsächliche Kurven-Hit-Test sollte mittelfristig in den Core.
7. Mittel: Canvas.svelte, App.svelte und Tauri lib.rs sind zu großen Sammelmodulen geworden
Aktuelle Größen:
frontend/src-tauri/src/lib.rs: etwa 1.680 Zeilen
Canvas.svelte: etwa 1.510 Zeilen
App.svelte: etwa 1.290 Zeilen
core/src/geo_ops.rs: über 1.050 Zeilen
core/src/state.rs: über 920 Zeilen
Das ist noch funktionsfähig, erschwert aber sichere Erweiterungen. Besonders Canvas.svelte enthält gleichzeitig:
Kamera
Raster und Lineale
Rendering
Auswahl
Resize
Bézier-Zeichnen
Knotenbearbeitung
Messen
Haltestege
Fillet-Markierungen
Bilder
Tastatursteuerung
Damit können Änderungen an einem Werkzeug leicht ein anderes beeinflussen.
Sinnvolle spätere Trennung:
canvas/camera.ts
canvas/render.ts
canvas/selection.ts
canvas/input.ts
canvas/tools/bezier.ts
canvas/tools/node.ts
canvas/tools/measure.ts
Im Rust-Core sollten Shape-Transformationen, Pfadoperationen und Arrange ebenfalls getrennte Verantwortlichkeiten bekommen.
8. Mittel: Transform-Inputs können bei nullbreiten Konturen ungültige Werte erzeugen
Die Seitenverhältniskopplung rechnet mit:
bbox[3] / bbox[2]
bbox[2] / bbox[3]
Siehe [TransformPanel.svelte (line 31)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/frontend/src/lib/TransformPanel.svelte:31).
Eine exakt horizontale oder vertikale Linie kann eine Breite beziehungsweise Höhe von null besitzen. Dann entstehen Infinity oder NaN.
Das sollte vor Freigabe der Transform-Leiste abgefangen werden. Für Linien braucht es eine definierte Semantik: Länge statt Rechteckgröße oder mindestens entkoppelte Skalierung auf der degenerierten Achse.
9. Mittel: Zwischenraum-Verteilung kann negative Abstände erzeugen
Bei „Zwischenräume angleichen“ wird der verfügbare Raum minus Gesamtgröße berechnet:
[arrange.rs (line 116)](/home/moshy/Dokumente/Coding/LuxiFer/luxifer/core/src/arrange.rs:116)
Überlappen die ausgewählten Objekte oder ist die Gesamtbreite größer als der äußere Bereich, wird der Abstand negativ. Das kann mathematisch gewollt sein, ist in der UI aber nicht erklärt.
Es sollte entschieden werden:
Überlappung als negativer Abstand ausdrücklich erlauben, oder
mindestens auf null begrenzen, oder
die Operation mit einer verständlichen Meldung ablehnen.
10. Mittel: Tauri-Commands und Frontend besitzen keine einheitliche Fehlerbehandlung
Viele Commands liefern direkt Scene, selbst wenn eine Operation ungültig oder wirkungslos ist. Das Frontend kann deshalb nicht unterscheiden zwischen:
erfolgreicher Änderung
No-op wegen falscher Auswahl
ungültiger Geometrie
internem Fehler
Gleichzeitig wird in Tauri sehr häufig Mutex::lock().unwrap() verwendet. Ein einzelner Panic während gehaltenem Lock kann den Mutex vergiften und weitere Commands zum Absturz bringen.
Langfristig sollte sich ein einheitliches Ergebnisformat etablieren:
Result<Scene, EditorError>
Dazu gehören stabile Fehlercodes und eine zentrale Toast-/Statusanzeige im Frontend.
11. Mittel: Clippy ist aktuell nicht vollständig grün
Die Tests und Frontend-Prüfungen laufen, aber cargo clippy --all-targets --all-features -- -D warnings scheitert bereits an vorhandenen Meldungen:
unnötiger Clone in core/examples/pf_check.rs
Feldzuweisung nach Default::default() in ui_settings.rs
Das sind keine schweren Laufzeitfehler. Problematisch ist aber, dass damit der vereinbarte Qualitäts-Gate nicht mehr zuverlässig zwischen bestehenden und neu eingeführten Warnungen unterscheiden kann.
12. Niedrig bis mittel: Aktuelle Änderungen sind noch nicht committed
Der Working Tree enthält derzeit Änderungen an:
Arrange-Core
Tauri-Commands
App-Layout
Arrange-Panel
Frontend-Bridge
neues Transform-Panel
Damit sind der funktionierende Bézier-Commit und die nachfolgenden Arrange-/Transform-Arbeiten zwar getrennt, die aktuelle zweite Einheit ist aber noch nicht dauerhaft gesichert.
Vor weiterer Featurearbeit sollte dieser Stand erst nach Korrektur der kritischen Punkte getestet und als eigener Commit abgeschlossen werden.
Zukünftige Ausbau-Blocker
Die folgenden Erweiterungen sollten nicht einfach auf den jetzigen Stand aufgesetzt werden:
Rotation in der Transform-Leiste
Benötigt rotationskorrekte Welt-Bounding-Boxen und zentrale Shape-Transformationen.

Freies Drehen per Canvas-Griff
Benötigt gemeinsamen Pivot, Gruppenbehandlung und korrekte Bézier-/Texttransformation.

Fortgeschrittener Node-Editor
Benötigt synchronisierte Bézier-Metadaten und Core-basierten Kurven-Hit-Test.

Objektinspektor mit X/Y/B/H
Benötigt eine kanonische Core-Auswahlbox statt mehrfacher Frontend-Berechnung.

Arrange für Gruppen und Text
Benötigt Arrange-Einheiten auf Basis von group_id.

Weitere editierbare Metadatenformen
Für Text, Bézier, Fillet und zukünftige parametrische Formen braucht es ein einheitliches Transformationsmodell.

GPU-Design-Canvas
Wird unnötig schwer, solange Rendering, Hit-Test und Werkzeugzustände in einer einzigen Canvas-Komponente gekoppelt sind.

Empfohlene Reihenfolge
Zentrale Shape-Transformationen für Translate, Scale und Mirror einführen.
Bézier-Metadaten bei allen Transformationen synchron halten.
Arrange auf echte Gruppen-Einheiten umstellen.
Rotationskorrekte Weltkontur und Bounding-Box definieren.
Auswahlbox aus dem Core an das Frontend liefern.
Verhalten skalierter editierbarer Texte festlegen.
Nullbreiten- und Nullhöhenfälle im Transform-Panel absichern.
Erst danach Rotation und Prozent-Skalierung ergänzen.
Große Frontend- und Tauri-Module schrittweise zerlegen.
Clippy-Gate wieder vollständig grün machen.
Der wichtigste nächste Schritt ist nicht zusätzliche UI, sondern eine einheitliche Transformationsschicht im Core. Sie beseitigt mehrere aktuelle Fehler gleichzeitig und schafft eine stabile Grundlage für Rotation, Objektinspektor, Gruppen-Arrange und weitere Pfadwerkzeuge.
