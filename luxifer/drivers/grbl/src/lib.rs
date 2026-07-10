//! GRBL/miniGRBL-Treiber: übersetzt den geräteunabhängigen [`JobPlan`] in G-Code.
//!
//! Kennt nur `luxifer-core` (ADR 0001). Der Core liefert Cut-Pfade in mm; dieser
//! Treiber macht daraus G-Code-Text (als UTF-8-Bytes).

use luxifer_core::{DriverError, JobAction, JobParams, JobPlan, Layer, LayerWork, MachineDriver};

/// Einstellungen der GRBL-Maschine.
#[derive(Debug, Clone)]
pub struct GrblConfig {
    /// S-Wert bei 100 % Leistung (GRBL `$30`, üblich 1000).
    pub max_power_s: f64,
    /// Laser-Modus: `M4` = dynamisch (Power skaliert mit Geschwindigkeit),
    /// `M3` = konstant. Für Gravur ist `M4` meist besser.
    pub dynamic_power: bool,
}

impl Default for GrblConfig {
    fn default() -> Self {
        Self {
            max_power_s: 1000.0,
            dynamic_power: true,
        }
    }
}

/// Der GRBL-Treiber.
#[derive(Debug, Clone, Default)]
pub struct GrblDriver {
    pub config: GrblConfig,
}

impl GrblDriver {
    pub fn new(config: GrblConfig) -> Self {
        Self { config }
    }

    /// Erzeugt den G-Code als String (praktisch für Tests/Vorschau).
    pub fn to_gcode(&self, plan: &JobPlan, _layers: &[Layer]) -> String {
        let mut g = String::new();
        let laser_on = if self.config.dynamic_power {
            "M4"
        } else {
            "M3"
        };

        // Präambel: mm, absolute Koordinaten, Laser initial aus.
        g.push_str("; LuxiFer — GRBL\n");
        g.push_str("G21\n"); // mm
        g.push_str("G90\n"); // absolut
        g.push_str("M5\n"); // Laser aus (sicher)
        g.push_str("G0 X0 Y0\n");

        for jl in &plan.layers {
            let s = (jl.power_pct / 100.0 * self.config.max_power_s).round();
            let feed = (jl.speed_mm_s * 60.0).round(); // mm/s → mm/min

            g.push_str(&format!(
                "; Ebene {} — {} mm/s, {} %\n",
                jl.layer_id, jl.speed_mm_s, jl.power_pct
            ));
            g.push_str(&format!("F{feed}\n"));

            let passes = jl.passes.max(1);
            for _pass in 0..passes {
                match &jl.work {
                    LayerWork::Cut { paths } => {
                        for path in paths {
                            if path.points.is_empty() {
                                continue;
                            }
                            // Zum Startpunkt fahren (Laser aus), dann Kontur brennen.
                            let (x0, y0) = path.points[0];
                            g.push_str("M5\n");
                            g.push_str(&format!("G0 X{} Y{}\n", num(x0), num(y0)));
                            g.push_str(&format!("{laser_on} S{}\n", num(s)));
                            for &(x, y) in &path.points[1..] {
                                g.push_str(&format!("G1 X{} Y{}\n", num(x), num(y)));
                            }
                            if path.closed {
                                g.push_str(&format!("G1 X{} Y{}\n", num(x0), num(y0)));
                            }
                            g.push_str("M5\n"); // Laser nach jedem Pfad aus
                        }
                    }
                    LayerWork::Fill { segments } => {
                        // Jedes horizontale Segment: anfahren, brennen, aus.
                        for seg in segments {
                            g.push_str("M5\n");
                            g.push_str(&format!("G0 X{} Y{}\n", num(seg.x0), num(seg.y)));
                            g.push_str(&format!("{laser_on} S{}\n", num(s)));
                            g.push_str(&format!("G1 X{} Y{}\n", num(seg.x1), num(seg.y)));
                            g.push_str("M5\n");
                        }
                    }
                    LayerWork::Raster { rows, .. } => {
                        // Bild-Raster: jeder An-Run einer Zeile wie ein Fill-Segment.
                        for row in rows {
                            for &(x0, x1) in &row.runs {
                                g.push_str("M5\n");
                                g.push_str(&format!("G0 X{} Y{}\n", num(x0), num(row.y)));
                                g.push_str(&format!("{laser_on} S{}\n", num(s)));
                                g.push_str(&format!("G1 X{} Y{}\n", num(x1), num(row.y)));
                                g.push_str("M5\n");
                            }
                        }
                    }
                }
            }
        }

        // Ende: Laser aus, zurück zum Ursprung.
        g.push_str("M5\n");
        g.push_str("G0 X0 Y0\n");
        g
    }
}

impl MachineDriver for GrblDriver {
    fn name(&self) -> &str {
        "GRBL"
    }

    fn compile_with(
        &self,
        plan: &JobPlan,
        layers: &[Layer],
        _params: &JobParams,
    ) -> Result<Vec<u8>, String> {
        if plan.is_empty() {
            return Err("Leerer Job — nichts zu lasern.".into());
        }
        // GRBL arbeitet in Maschinenkoordinaten; „Starten von"/Anker werden
        // (vorerst) nicht umgesetzt — der Startmodus ist ein Ruida-Konzept.
        Ok(self.to_gcode(plan, layers).into_bytes())
    }

    fn actions(&self) -> Vec<JobAction> {
        // Streamen braucht noch den seriellen Transport; Export geht schon.
        vec![JobAction::ExportFile, JobAction::StreamGcode]
    }

    fn run_action(
        &self,
        action: JobAction,
        plan: &JobPlan,
        layers: &[Layer],
        params: &JobParams,
    ) -> Result<String, DriverError> {
        match action {
            JobAction::ExportFile => {
                let bytes = self
                    .compile_with(plan, layers, params)
                    .map_err(DriverError::Transport)?;
                Ok(format!("G-Code erzeugt ({} Byte).", bytes.len()))
            }
            // Serielles Streamen kommt mit dem GRBL-Transport (späterer Schritt).
            _ => Err(DriverError::NotSupported),
        }
    }
}

/// Zahl kompakt formatieren (max. 3 Nachkommastellen, keine überflüssigen Nullen).
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luxifer_core::{AppState, Geo};

    fn plan_one_rect() -> (JobPlan, Vec<Layer>) {
        let mut st = AppState::new();
        st.add_shape(Geo::Rect {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 30.0,
        });
        st.layers[0].speed_mm_s = 50.0;
        st.layers[0].power_pct = 40.0;
        let plan = JobPlan::from_shapes(&st.shapes, &st.layers);
        (plan, st.layers)
    }

    #[test]
    fn erzeugt_gueltige_praeambel_und_feed() {
        let (plan, layers) = plan_one_rect();
        let g = GrblDriver::default().to_gcode(&plan, &layers);
        assert!(g.contains("G21")); // mm
        assert!(g.contains("G90")); // absolut
        assert!(g.contains("F3000")); // 50 mm/s * 60
        assert!(g.contains("S400")); // 40% von 1000
        assert!(g.trim_end().ends_with("G0 X0 Y0")); // sauberer Abschluss
    }

    #[test]
    fn faehrt_kontur_ab_und_schliesst() {
        let (plan, layers) = plan_one_rect();
        let g = GrblDriver::default().to_gcode(&plan, &layers);
        assert!(g.contains("G0 X10 Y10")); // Startpunkt anfahren
        assert!(g.contains("G1 X30 Y10")); // erste Kante
        assert!(g.contains("M4 S400")); // Laser an (dynamisch)
                                        // Geschlossenes Rechteck kehrt zum Start zurück.
        let last_g1 = g.matches("G1 X10 Y10").count();
        assert!(last_g1 >= 1);
    }

    #[test]
    fn passes_wiederholen_die_kontur() {
        let (mut plan, mut layers) = plan_one_rect();
        layers[0].passes = 3;
        // Plan neu bauen wäre nötig, aber passes steckt im JobLayer:
        plan.layers[0].passes = 3;
        let g = GrblDriver::default().to_gcode(&plan, &layers);
        // Startpunkt-Anfahrt kommt pro Pass einmal vor.
        assert_eq!(g.matches("G0 X10 Y10").count(), 3);
    }

    #[test]
    fn m3_bei_konstanter_power() {
        let (plan, layers) = plan_one_rect();
        let d = GrblDriver::new(GrblConfig {
            dynamic_power: false,
            ..Default::default()
        });
        let g = d.to_gcode(&plan, &layers);
        assert!(g.contains("M3 S400"));
        assert!(!g.contains("M4 S"));
    }

    #[test]
    fn leerer_job_ist_fehler() {
        let plan = JobPlan {
            layers: vec![],
            bbox: None,
        };
        assert!(GrblDriver::default().compile(&plan, &[]).is_err());
    }
}
