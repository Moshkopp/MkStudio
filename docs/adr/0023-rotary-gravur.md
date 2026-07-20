# ADR 0023: Rotary-Gravur

- Status: **Entwurf — nicht umgesetzt, zur Prüfung**
- Datum: 2026-07-20
- Betrifft: studio-core (Job-Kompilierung), Treiber (Ruida), Application,
  Laserprofile, Laserpanel
- Baut auf: ADR 0021 (Zusatzachsen/Jog/Rotary-Modi), ADR 0022 (Rotary-Bauarten
  und Achsenkalibrierung)

## Kontext

ADR 0021 legt die drei Rotary-Modi (`Aus`, `UAchse`, `YAchse`) und ihr
**Jog**-Verhalten fest. ADR 0022 liefert das **Fachmodell** (Chuck/Roller,
`circumference_mm`, `steps_per_mm`) und die Achsenkalibrierung. Beide ADRs
verweisen die eigentliche **Gravur** ausdrücklich auf ein eigenes ADR — dieses.

Die offene Frage ist nicht, wie sich ein Rotary dreht, sondern **wann und wo aus
einem flachen Job ein Rotary-Job wird**. Der Job-Pfad ist heute durchgängig
zweidimensional: Shapes in mm → `JobPlan` → Treiber → Controller. Ein Rotary
ersetzt eine der beiden Achsen durch eine Drehung.

### Der Kern: zwei Modi, zwei völlig verschiedene Verantwortungen

Die beiden Rotary-Modi aus ADR 0021 unterscheiden sich in der Gravur **nicht
graduell, sondern grundsätzlich** — und zwar darin, *wer* rechnet:

| | `YAchse` | `UAchse` |
|---|---|---|
| Wer skaliert | **Der Controller** (`0x0226` + `0x021F`/`0x0221`) | **Studio** |
| Was Studio sendet | ein ganz normaler X/Y-Job | X + U statt X/Y |
| Was Studio wissen muss | dass die Register stimmen | die volle Rotary-Physik |
| Risiko | Doppelskalierung | falsche Abwicklung |

Daraus folgt die zentrale Feststellung dieses Entwurfs:

> **Im Modus `YAchse` darf Studio die Bewegung nicht selbst umrechnen.** Der
> Controller tut es bereits. Eine zusätzliche app-seitige Skalierung ergäbe eine
> doppelte Umrechnung — der Job führe um den Faktor der Rotary-Skalierung falsch.

Das ist der gefährlichste denkbare Fehler in diesem Bereich, und er entsteht
gerade dann, wenn man „Rotary-Gravur" als *einen* Fall behandelt.

## Entscheidung (Vorschlag)

**(A) Rotary-Gravur ist kein eigener Job-Typ, sondern eine Eigenschaft der
Kompilierung. (B) Im Modus `YAchse` bleibt der Job unverändert — Studio schreibt
nur die Controller-Register konsistent. (C) Nur im Modus `UAchse` rechnet der
Core eine Achse in Drehung um, über das Fachmodell aus ADR 0022. (D) Die
Umrechnung liegt im Core, nicht im Treiber und nicht in der UI.**

### (A) Kein eigener Job-Typ

Ein Rotary-Job ist derselbe `JobPlan` wie ein flacher Job. Es gibt **keinen**
`RotaryJobPlan`. Was sich ändert, ist ausschließlich, wie eine Achse beim
Kompilieren interpretiert wird. Damit bleiben Vorschau, Ausführungsspur (ADR
0015), Materialrezepte (ADR 0019) und Nullpunkte (ADR 0020) unangetastet.

### (B) `YAchse`: Studio rechnet nicht

Studio kompiliert einen normalen X/Y-Job. Zusätzlich stellt es sicher, dass die
Controller-Register zum Profil passen:

- `0x0226 rotary_enable` = 1
- `0x021F pulses_per_rot` und `0x0221 rotary_diameter` entsprechend dem Rotary
  aus dem Profil (ADR 0022)

Offen zur Klärung: ob Studio diese Register **schreibt** (Risiko: überschreibt
Nutzereinstellungen) oder nur **prüft und warnt** (Risiko: Nutzer graviert mit
falscher Skalierung). Der Entwurf neigt zu *prüfen und warnen*, weil das
Schreiben von Registern ohne Not eine fremde Maschinenkonfiguration verändert.

### (C) `UAchse`: der Core rechnet

Hier ersetzt U die Y-Achse. Der Core rechnet die Y-Koordinate jedes Pfadpunkts
in eine U-Strecke um, über `Rotary::steps_per_mm()` bzw. die Abwicklung aus ADR
0022. Chuck und Roller unterscheiden sich dabei genau so, wie ADR 0022 es
festlegt — der Gravur-Pfad kennt den Unterschied nicht, er fragt das Modell.

Offen zur Klärung: ob U in mm-Abwicklung oder in Grad ausgedrückt wird. Der
Entwurf neigt zu **mm-Abwicklung**, weil dann Geometrie, Vorschub und
Materialparameter in derselben Einheit bleiben und die Bauart im Modell
gekapselt ist.

### (D) Verortung

- **Core**: die Umrechnung Y→U als reine Funktion über dem Rotary-Modell,
  testbar ohne Gerät. Keine Register, kein Treiberwissen.
- **Treiber**: bildet die umgerechneten Werte auf seine Ausgabe ab, wie bisher.
- **Application**: wählt anhand des Profil-Modus, welcher Weg gilt, und
  verweigert die Ausführung bei unstimmiger Konfiguration.
- **Native**: zeigt den Modus an und warnt sichtbar, wenn ein Rotary-Job ansteht.

## Offene Fragen (vor Umsetzung zu klären)

1. **Register schreiben oder nur prüfen?** (§B) — betrifft fremde Maschinen.
2. **U in mm oder Grad?** (§C) — betrifft Materialparameter und Vorschub.
3. **Was passiert mit der zweiten Achse?** Bei `UAchse` bleibt Y physisch
   vorhanden. Wird sie im Rotary-Job gesperrt, oder darf ein Job X/Y/U mischen?
4. **Bettgrenzen.** Eine Drehung hat keine Begrenzung, die flache Y-Achse schon.
   Wie wird die Arbeitsbereichsprüfung im Rotary-Modus umgestellt?
5. **Vorschau.** Zeigt die Vorschau die abgewickelte Fläche (flach) oder deutet
   sie die Rundung an? Der Entwurf neigt zu flach-abgewickelt.
6. **Umfang-Überlauf.** Was, wenn das Objekt länger als der Umfang ist — Abbruch,
   Warnung oder Wickeln?

## Konsequenzen

**Positiv**

- Die gefährliche Doppelskalierung im `YAchse`-Modus ist ausdrücklich
  ausgeschlossen statt implizit vermieden.
- Rotary-Gravur erbt Vorschau, Spur und Materiallogik, statt sie zu duplizieren.
- Die Physik bleibt an einer Stelle (ADR 0022), die Gravur fragt sie nur ab.

**Aufwand / Risiko**

- Der Job-Kompilierungspfad im Core bekommt eine achsabhängige Verzweigung —
  die Stelle, an der ein Fehler direkt Hardware bewegt. Braucht Tests mit
  bytegenauen Erwartungen, analog zum vorhandenen Jog-Test.
- Die offenen Fragen 3–6 sind keine Details: jede kann die Umsetzung ändern.
- **Nichts davon ist an Hardware verifiziert.** Insbesondere die Annahme, dass
  der Controller im `YAchse`-Modus vollständig selbst skaliert, stammt aus der
  Registeranalyse (ADR 0021 §D) und ist nicht gemessen.

## Nicht Teil dieser Entscheidung

- GRBL/FluidNC-Rotary (A-Achse) — das Modell ist vorbereitet, die Abbildung
  folgt, wenn ein solcher Treiber produktiv wird.
- Die U/Z-Enable-Bit-Dekodierung (weiterhin offen aus ADR 0021).
