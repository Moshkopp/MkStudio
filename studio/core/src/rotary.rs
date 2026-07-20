//! Rotary-Fachmodell und Achsenkalibrierung (ADR 0022).
//!
//! Beides ist bewusst **geräteneutral**: dieses Modul kennt weder Register noch
//! Treiber, nur die Physik bzw. die reine Rechnung. Die Abbildung auf konkrete
//! Controller-Register (Ruida `*_step_length`, GRBL `$10x`, FluidNC-Config)
//! liegt im jeweiligen Treiber.

use serde::{Deserialize, Serialize};

/// Bauart eines Rotary-Aufsatzes. Chuck und Roller sind **nicht** derselbe Fall
/// mit anderem Wert, sondern zwei Skalierungsmodelle (ADR 0022 §A).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RotaryKind {
    /// Futter: das Objekt sitzt im Futter und dreht direkt mit. Der abgewickelte
    /// Umfang hängt am Objektdurchmesser.
    Chuck { object_diameter_mm: f64 },
    /// Abroller: das Objekt liegt auf zwei Walzen, die Walze schiebt es am
    /// Auflagepunkt. Maßgeblich ist der **Walzendurchmesser** — das Objekt darf
    /// beliebig dick sein, eine Walzenumdrehung schiebt immer dieselbe Strecke.
    Roller { roller_diameter_mm: f64 },
}

impl Default for RotaryKind {
    fn default() -> Self {
        Self::Chuck {
            object_diameter_mm: 50.0,
        }
    }
}

impl RotaryKind {
    /// Durchmesser des treibenden Elements in mm — beim Chuck das Objekt, beim
    /// Roller die Walze.
    pub fn driving_diameter_mm(self) -> f64 {
        match self {
            Self::Chuck { object_diameter_mm } => object_diameter_mm,
            Self::Roller { roller_diameter_mm } => roller_diameter_mm,
        }
    }
}

/// Rotary-Aufsatz eines Lasers: Bauart plus Schritte pro Umdrehung des
/// treibenden Elements.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rotary {
    #[serde(default)]
    pub kind: RotaryKind,
    /// Motor-/Getriebeschritte pro voller Umdrehung.
    #[serde(default = "default_steps_per_rev")]
    pub steps_per_rev: f64,
}

fn default_steps_per_rev() -> f64 {
    10_000.0
}

impl Default for Rotary {
    fn default() -> Self {
        Self {
            kind: RotaryKind::default(),
            steps_per_rev: default_steps_per_rev(),
        }
    }
}

impl Rotary {
    /// Abgewickelter Umfang in mm pro Umdrehung des treibenden Elements.
    /// Chuck: π × Objekt-Ø, Roller: π × Walzen-Ø.
    pub fn circumference_mm(&self) -> f64 {
        std::f64::consts::PI * self.kind.driving_diameter_mm()
    }

    /// Schritte pro mm Abwicklung. `None`, wenn der Durchmesser unbrauchbar ist
    /// (≤ 0 oder nicht endlich) — dann gibt es keine sinnvolle Skalierung, und
    /// ein stillschweigendes Ersatzergebnis wäre eine falsche Bewegung.
    pub fn steps_per_mm(&self) -> Option<f64> {
        let circumference = self.circumference_mm();
        if !circumference.is_finite() || circumference <= 0.0 || !self.steps_per_rev.is_finite() {
            return None;
        }
        Some(self.steps_per_rev / circumference)
    }
}

/// Fehler der Achsenkalibrierung — bewusst getrennt statt eines stillen
/// Ersatzwerts: ein falscher Steps-Wert im Controller bewegt echte Hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationError {
    /// Die gemessene Ist-Strecke ist 0 (oder praktisch 0) — daraus lässt sich
    /// kein Verhältnis bilden.
    MeasuredZero,
    /// Soll, Ist oder der aktuelle Wert ist negativ, 0 oder nicht endlich.
    InvalidInput,
}

impl std::fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MeasuredZero => write!(f, "Die gemessene Strecke darf nicht 0 sein."),
            Self::InvalidInput => write!(f, "Soll, Ist und aktueller Wert müssen positiv sein."),
        }
    }
}

impl std::error::Error for CalibrationError {}

/// Unterhalb dieser gemessenen Strecke (mm) gilt das Ist als „0": das
/// Verhältnis würde ins Unermessliche laufen.
const MIN_MEASURED_MM: f64 = 1e-6;

/// Neue **Schrittlänge** einer Achse aus Soll/Ist: `neu = alt × Ist / Soll`.
///
/// Die Richtung ist an Hardware verifiziert und leicht zu verwechseln: die
/// Schrittlänge ist die Strecke **pro Schritt** (Ruida: µm, `*_step_length`).
/// Für eine gewünschte Strecke fährt der Controller `Strecke / Schrittlänge`
/// Schritte — die gefahrene Strecke ist also **umgekehrt** proportional zum
/// eingetragenen Wert:
///
/// - fährt zu **weit** (Ist > Soll) → Schrittlänge war zu **klein** → sie muss
///   **steigen**, damit weniger Schritte gefahren werden.
///
/// Beispiel (an einem Ruida gegengeprüft): 10,6667 µm, Soll 10 mm, Ist 25 mm
/// → 26,6667 µm.
///
/// ACHTUNG: ADR 0022 §B notiert `neu = alt × Soll / Ist`. Das gilt für die
/// umgekehrte Größe „Schritte pro mm" (GRBL `$100`), **nicht** für eine
/// Schrittlänge. Ein Treiber, der Schritte/mm führt, muss den Kehrwert
/// verwenden.
pub fn calibrated_step_length(
    current_step_length: f64,
    target_mm: f64,
    measured_mm: f64,
) -> Result<f64, CalibrationError> {
    if !current_step_length.is_finite()
        || !target_mm.is_finite()
        || !measured_mm.is_finite()
        || current_step_length <= 0.0
        || target_mm <= 0.0
        || measured_mm < 0.0
    {
        return Err(CalibrationError::InvalidInput);
    }
    if measured_mm < MIN_MEASURED_MM {
        return Err(CalibrationError::MeasuredZero);
    }
    Ok(current_step_length * (measured_mm / target_mm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chuck_wickelt_den_objektumfang_ab() {
        let rotary = Rotary {
            kind: RotaryKind::Chuck {
                object_diameter_mm: 10.0,
            },
            steps_per_rev: 1_000.0,
        };
        assert!((rotary.circumference_mm() - std::f64::consts::PI * 10.0).abs() < 1e-9);
        let per_mm = rotary.steps_per_mm().expect("gültiger Durchmesser");
        assert!((per_mm - 1_000.0 / (std::f64::consts::PI * 10.0)).abs() < 1e-9);
    }

    #[test]
    fn roller_ignoriert_den_objektdurchmesser() {
        // Kernaussage von ADR 0022: bei der Walze zählt nur der Walzen-Ø. Zwei
        // Aufbauten mit gleicher Walze liefern dieselbe Abwicklung, egal wie
        // dick das Objekt ist — deshalb trägt RotaryKind::Roller den Objekt-Ø
        // gar nicht erst.
        let rotary = Rotary {
            kind: RotaryKind::Roller {
                roller_diameter_mm: 20.0,
            },
            steps_per_rev: 1_000.0,
        };
        assert!((rotary.circumference_mm() - std::f64::consts::PI * 20.0).abs() < 1e-9);
    }

    #[test]
    fn chuck_und_roller_sind_verschiedene_modelle() {
        // Gleicher Zahlenwert, andere Bedeutung: 20 mm Objekt im Futter wickelt
        // anders ab als ein Objekt auf einer 20-mm-Walze nur dann NICHT, wenn
        // beide Durchmesser zufällig gleich sind. Der Test hält fest, dass die
        // Bauart und nicht der Zahlenwert entscheidet.
        let chuck = Rotary {
            kind: RotaryKind::Chuck {
                object_diameter_mm: 80.0,
            },
            steps_per_rev: 1_000.0,
        };
        let roller = Rotary {
            kind: RotaryKind::Roller {
                roller_diameter_mm: 20.0,
            },
            steps_per_rev: 1_000.0,
        };
        assert!(chuck.circumference_mm() > roller.circumference_mm());
    }

    #[test]
    fn unbrauchbarer_durchmesser_liefert_keine_skalierung() {
        for diameter in [0.0, -5.0, f64::NAN] {
            let rotary = Rotary {
                kind: RotaryKind::Chuck {
                    object_diameter_mm: diameter,
                },
                steps_per_rev: 1_000.0,
            };
            assert!(
                rotary.steps_per_mm().is_none(),
                "Ø {diameter} muss None sein"
            );
        }
    }

    #[test]
    fn schrittlaenge_waechst_wenn_die_achse_zu_weit_faehrt() {
        // An echter Hardware gegengeprüfter Anker: U-Achse mit 10,6667 µm,
        // Soll 10 mm, gemessen 25 mm → 26,6667 µm. Die Schrittlänge STEIGT.
        // Ein früherer Stand rechnete Soll/Ist und fuhr dadurch noch weiter;
        // dieser Test hält die Richtung fest.
        let neu = calibrated_step_length(10.6667, 10.0, 25.0).expect("gültige Eingabe");
        assert!(
            (neu - 26.666_75).abs() < 1e-3,
            "erwartet ≈26,6667 µm, war {neu}"
        );
        assert!(
            neu > 10.6667,
            "zu weit gefahren muss die Schrittlänge erhöhen"
        );
    }

    #[test]
    fn schrittlaenge_faellt_wenn_die_achse_zu_kurz_faehrt() {
        // Gegenprobe in die andere Richtung: Soll 10, real nur 5 gefahren.
        let neu = calibrated_step_length(10.0, 10.0, 5.0).expect("gültige Eingabe");
        assert!((neu - 5.0).abs() < 1e-9);
        assert!(neu < 10.0, "zu kurz gefahren muss die Schrittlänge senken");
    }

    #[test]
    fn kalibrierung_ist_bei_treffer_neutral() {
        let neu = calibrated_step_length(1_000.0, 10.0, 10.0).expect("gültige Eingabe");
        assert!((neu - 1_000.0).abs() < 1e-9);
    }

    #[test]
    fn kalibrierung_trifft_in_einem_schritt() {
        // Maschinenmodell: der Controller fährt `Strecke / Schrittlänge`
        // Schritte, die gefahrene Strecke ist also UMGEKEHRT proportional zur
        // eingetragenen Schrittlänge. Ein Durchgang muss exakt treffen.
        let soll = 10.0;
        let start = 10.6667;
        // Bei `start` fährt die Maschine 25 mm statt 10 (echte Messung).
        let ist_bei = |len: f64| 25.0 * (start / len);

        let neu = calibrated_step_length(start, soll, ist_bei(start)).expect("gültige Eingabe");
        let ist = ist_bei(neu);
        assert!(
            (ist - soll).abs() < 1e-9,
            "nach einem Schritt muss Ist ({ist}) dem Soll entsprechen"
        );
        // Eine weitere Runde auf bereits richtigem Wert ändert nichts mehr.
        let erneut = calibrated_step_length(neu, soll, ist).expect("gültige Eingabe");
        assert!((erneut - neu).abs() < 1e-9, "Kalibrierung ist nicht stabil");
    }

    #[test]
    fn kalibrierung_lehnt_null_messung_ab() {
        assert_eq!(
            calibrated_step_length(1_000.0, 10.0, 0.0),
            Err(CalibrationError::MeasuredZero)
        );
    }

    #[test]
    fn kalibrierung_lehnt_unbrauchbare_eingaben_ab() {
        for (current, soll, ist) in [
            (0.0, 10.0, 10.0),
            (-1.0, 10.0, 10.0),
            (1_000.0, 0.0, 10.0),
            (1_000.0, -10.0, 10.0),
            (1_000.0, 10.0, -10.0),
            (f64::NAN, 10.0, 10.0),
            (1_000.0, f64::INFINITY, 10.0),
        ] {
            assert_eq!(
                calibrated_step_length(current, soll, ist),
                Err(CalibrationError::InvalidInput),
                "({current}, {soll}, {ist}) muss abgelehnt werden"
            );
        }
    }
}
