//! GRBL/miniGRBL-Treiber: übersetzt den geräteunabhängigen [`JobPlan`] in G-Code.
//!
//! Kennt nur `studio-core` (ADR 0001). Der Core liefert Cut-Pfade in mm; dieser
//! Treiber macht daraus G-Code-Text (als UTF-8-Bytes).

mod protocol;
mod transport;

use studio_core::{DriverError, JobAction, JobParams, JobPlan, Layer, LayerWork, MachineDriver};

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
#[derive(Default)]
pub struct GrblDriver {
    pub config: GrblConfig,
    transport: Option<transport::SerialTransport>,
}

#[derive(Clone, Copy)]
struct GrblMotion {
    from: (f64, f64),
    to: (f64, f64),
    kind: studio_core::ExecutionKind,
}

fn push_grbl_travel(head: &mut (f64, f64), to: (f64, f64), motions: &mut Vec<GrblMotion>) {
    motions.push(GrblMotion {
        from: *head,
        to,
        kind: studio_core::ExecutionKind::Travel,
    });
    *head = to;
}

fn grbl_motion_program(plan: &JobPlan) -> Vec<(usize, f64, f64, Vec<GrblMotion>)> {
    use studio_core::ExecutionKind as K;
    let mut head = (0.0, 0.0);
    let mut program = Vec::new();
    for layer in &plan.layers {
        let mut motions = Vec::new();
        for _ in 0..layer.passes.max(1) {
            match &layer.work {
                LayerWork::Cut { paths } => {
                    for path in paths {
                        let Some(&start) = path.points.first() else {
                            continue;
                        };
                        push_grbl_travel(&mut head, start, &mut motions);
                        for pair in path.points.windows(2) {
                            motions.push(GrblMotion {
                                from: pair[0],
                                to: pair[1],
                                kind: K::Cut,
                            });
                            head = pair[1];
                        }
                        if path.closed {
                            let last = *path.points.last().unwrap();
                            motions.push(GrblMotion {
                                from: last,
                                to: start,
                                kind: K::Cut,
                            });
                            head = start;
                        }
                    }
                }
                LayerWork::Fill { segments } => {
                    for segment in segments {
                        let from = (segment.x0, segment.y);
                        let to = (segment.x1, segment.y);
                        push_grbl_travel(&mut head, from, &mut motions);
                        motions.push(GrblMotion {
                            from,
                            to,
                            kind: K::Fill,
                        });
                        head = to;
                    }
                }
                LayerWork::Raster { rows, .. } => {
                    for row in rows {
                        for &(x0, x1) in &row.runs {
                            let from = (x0, row.y);
                            let to = (x1, row.y);
                            push_grbl_travel(&mut head, from, &mut motions);
                            motions.push(GrblMotion {
                                from,
                                to,
                                kind: K::Raster,
                            });
                            head = to;
                        }
                    }
                }
            }
        }
        program.push((layer.layer_id, layer.speed_mm_s, layer.power_pct, motions));
    }
    if let Some(last) = program.last_mut() {
        last.3.push(GrblMotion {
            from: head,
            to: (0.0, 0.0),
            kind: studio_core::ExecutionKind::Travel,
        });
    }
    program
}

impl GrblDriver {
    pub fn new(config: GrblConfig) -> Self {
        Self {
            config,
            transport: None,
        }
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
        g.push_str(&format!(
            "; {} — GRBL\n",
            studio_core::branding::PRODUCT_NAME
        ));
        g.push_str("G21\n"); // mm
        g.push_str("G90\n"); // absolut
        g.push_str("M5\n"); // Laser aus (sicher)
        g.push_str("G0 X0 Y0\n");

        for (layer_id, speed, power, motions) in grbl_motion_program(plan) {
            let s = (power / 100.0 * self.config.max_power_s).round();
            let feed = (speed * 60.0).round();

            g.push_str(&format!(
                "; Ebene {} — {} mm/s, {} %\n",
                layer_id, speed, power
            ));
            g.push_str(&format!("F{feed}\n"));

            let mut laser_active = false;
            for motion in motions {
                if motion.kind == studio_core::ExecutionKind::Travel {
                    if laser_active {
                        g.push_str("M5\n");
                        laser_active = false;
                    }
                    g.push_str(&format!("G0 X{} Y{}\n", num(motion.to.0), num(motion.to.1)));
                } else {
                    if !laser_active {
                        g.push_str(&format!("{laser_on} S{}\n", num(s)));
                        laser_active = true;
                    }
                    g.push_str(&format!("G1 X{} Y{}\n", num(motion.to.0), num(motion.to.1)));
                }
            }
            if laser_active {
                g.push_str("M5\n");
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

    fn export_extension(&self) -> &'static str {
        "gcode"
    }

    fn capabilities(&self) -> studio_core::DriverCapabilities {
        studio_core::DriverCapabilities {
            position_read: true,
            ..Default::default()
        }
    }

    fn execution_trace(
        &self,
        plan: &JobPlan,
        _layers: &[Layer],
        _params: &JobParams,
    ) -> Result<studio_core::ExecutionTrace, String> {
        use studio_core::TraceBuilder;
        let mut trace = TraceBuilder::new(false);
        trace.set_head((0.0, 0.0));
        for (layer_id, _, _, motions) in grbl_motion_program(plan) {
            for motion in motions {
                if motion.kind == studio_core::ExecutionKind::Travel {
                    trace.travel_to(motion.to, layer_id);
                } else {
                    trace.work(
                        motion.from,
                        motion.to,
                        motion.from,
                        motion.to,
                        motion.kind,
                        layer_id,
                    );
                }
            }
        }
        Ok(trace.finish())
    }

    fn connect(&mut self, connection: &studio_core::Connection) -> Result<(), DriverError> {
        let studio_core::Connection::Seriell { port, baud } = connection else {
            return Err(DriverError::Transport(
                "GRBL benötigt eine serielle Verbindung.".into(),
            ));
        };
        if self
            .transport
            .as_ref()
            .is_some_and(|transport| transport.matches(port, *baud))
        {
            return Ok(());
        }
        self.transport = None;
        self.transport = Some(transport::SerialTransport::connect(port, *baud)?);
        Ok(())
    }

    fn disconnect(&mut self) {
        self.transport = None;
    }

    fn console_snapshot(&self) -> Vec<studio_core::DriverConsoleLine> {
        self.transport
            .as_ref()
            .map(transport::SerialTransport::console_snapshot)
            .unwrap_or_default()
    }

    fn status(&self) -> Result<studio_core::MachineStatus, DriverError> {
        let transport = self.transport.as_ref().ok_or(DriverError::NotConnected)?;
        let status = transport.status()?;
        let position = status
            .machine_position
            .or(status.work_position)
            .unwrap_or_default();
        Ok(studio_core::MachineStatus {
            is_running: matches!(status.state.as_str(), "Run" | "Jog" | "Home"),
            is_paused: status.state.starts_with("Hold") || status.state.starts_with("Door"),
            pos_x_mm: position[0],
            pos_y_mm: position[1],
            pos_z_mm: Some(position[2]),
            pos_u_mm: None,
            rotary_on_y: false,
        })
    }

    fn send_job(&self, bytes: &[u8]) -> Result<(), DriverError> {
        let transport = self.transport.as_ref().ok_or(DriverError::NotConnected)?;
        transport.send_program(bytes)?;
        Ok(())
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
            JobAction::StreamGcode => {
                let bytes = self
                    .compile_with(plan, layers, params)
                    .map_err(DriverError::Transport)?;
                let transport = self.transport.as_ref().ok_or(DriverError::NotConnected)?;
                let lines = transport.send_program(&bytes)?;
                Ok(format!("G-Code gesendet ({lines} Befehle)."))
            }
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
    use studio_core::{AppState, Geo};

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
    fn maschineneinstellungen_bleiben_nicht_unterstuetzt() {
        let driver = GrblDriver::default();
        assert!(!driver.capabilities().machine_settings);
        assert_eq!(
            driver.read_machine_settings().unwrap_err(),
            DriverError::NotSupported
        );
    }

    /// Expliziter, standardmäßig übersprungener Hardware-Smoke-Test. Sendet
    /// ausschließlich Handshake, `$I` und `?`, niemals Bewegung oder Laser.
    #[test]
    #[ignore = "benötigt einen ausdrücklich angeschlossenen GRBL-Controller"]
    fn hardware_serieller_handshake_und_status() {
        let port = std::env::var("GRBL_PORT").expect("GRBL_PORT fehlt");
        let baud = std::env::var("GRBL_BAUD")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(115_200);
        let mut driver = GrblDriver::default();
        driver
            .connect(&studio_core::Connection::Seriell { port, baud })
            .expect("Handshake");
        let status = driver.status().expect("Status");
        assert!(status.pos_x_mm.is_finite());
        assert!(status.pos_y_mm.is_finite());
        driver.disconnect();
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
