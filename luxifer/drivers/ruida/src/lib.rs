//! Ruida-Treiber: übersetzt den geräteunabhängigen [`JobPlan`] in einen
//! vollständigen Ruida-Binärjob (RDC6445G).
//!
//! Kennt nur `luxifer-core` (ADR 0001). Job-Rahmen (Preamble, Layer-Config,
//! Settings-Block, Geometrie, Trailer) folgt der HW-verifizierten
//! ThorBurn-Referenz. Kodierung siehe [`protocol`].
//!
//! Start-Modus ist derzeit „Absolut" (kein Anker-Offset). Andere Startmodi und
//! der Fokus-Z-Move sind Ausbaustufen.

pub mod protocol;
pub mod scan_offset;
pub mod transport;

pub use scan_offset::{ScanOffset, ScanOffsetPoint};
pub use transport::{RuidaTransport, TransportError};

use luxifer_core::{
    Anchor, DriverError, JobAction, JobLayer, JobParams, JobPlan, Layer, LayerWork, MachineDriver,
    MachineStatus, StartMode,
};
use protocol::*;

/// Maschinenspezifische Ruida-Kalibrierung. Der Treiber trägt sie als Zustand
/// (Variante A, ADR 0006): einmal bei der Erzeugung gesetzt, nicht pro Aufruf.
/// Später wird sie aus dem aktiven Laser-Profil (ADR 0007) gespeist.
#[derive(Debug, Clone, Default)]
pub struct RuidaConfig {
    /// Geschwindigkeitsabhängige Reversal-Korrektur fürs bidirektionale Rastern.
    pub scan_offset: ScanOffset,
}

/// Der Ruida-Treiber.
///
/// Hält die Kalibrierung (Profil bei der Erzeugung, ADR 0006) und — nach
/// `connect` — die offene UDP-Verbindung. Nicht `Clone`, da der Socket
/// exklusiv ist.
#[derive(Debug, Default)]
pub struct RuidaDriver {
    pub config: RuidaConfig,
    transport: Option<RuidaTransport>,
}

impl RuidaDriver {
    /// Treiber mit Kalibrierung erzeugen (Profil bei der Erzeugung, ADR 0006).
    pub fn new(config: RuidaConfig) -> Self {
        Self {
            config,
            transport: None,
        }
    }

    /// Treiber aus einem Laser-Profil (ADR 0007) erzeugen: übernimmt die
    /// Scan-Offset-Kalibrierung des Profils in die Treiber-Config.
    pub fn from_profile(profile: &luxifer_core::LaserProfile) -> Self {
        let scan_offset = ScanOffset {
            enabled: profile.scan_offset.enabled,
            points: profile
                .scan_offset
                .points
                .iter()
                .map(|p| ScanOffsetPoint {
                    speed_mm_s: p.speed_mm_s,
                    offset_mm: p.offset_mm,
                })
                .collect(),
        };
        Self::new(RuidaConfig { scan_offset })
    }

    fn transport(&self) -> Result<&RuidaTransport, DriverError> {
        self.transport.as_ref().ok_or(DriverError::NotConnected)
    }
}

impl RuidaDriver {
    /// Fährt einen geschlossenen Punktzug (µm) als Rahmen ab, je Startmodus/
    /// Anker verschoben (Referenzlogik): Bei „Aktuelle Position"/
    /// „Benutzerursprung" landet der Ankerpunkt der Rahmen-BBox auf der
    /// Kopfposition bzw. dem Benutzerursprung. Die Sequenz nullt vorher die
    /// Leistungsregister (Rahmen darf nie brennen) und kehrt am Ende zur
    /// Ausgangsposition zurück — alles in EINEM Paket.
    fn drive_frame(
        &self,
        mut pts: Vec<(i32, i32)>,
        speed_mm_s: f64,
        params: &JobParams,
    ) -> Result<(), DriverError> {
        let t = self.transport()?;
        // Ausgangsposition lesen (Rückkehrpunkt; bei „Aktuelle Position" auch
        // der Referenzpunkt).
        let cx = read_reg(t, ADDR_POS_X)?;
        let cy = read_reg(t, ADDR_POS_Y)?;
        let reference = match params.start_mode {
            StartMode::Absolut => None,
            StartMode::AktuellePosition => Some((cx, cy)),
            StartMode::Benutzerursprung => {
                Some((read_reg(t, ADDR_ORIGIN_X)?, read_reg(t, ADDR_ORIGIN_Y)?))
            }
        };
        shift_frame_points(&mut pts, reference, params.anchor);

        // Speed 0 → Leistung nullen → zur Startecke → Speed → Segmente →
        // Speed 0 → zurück zur Ausgangsposition.
        let mut seq = cmd_set_speed(0.0);
        for reg in [0x01u8, 0x02, 0x21, 0x22] {
            seq.extend_from_slice(&[0xC6, reg, 0x00, 0x00]);
        }
        seq.extend(cmd_rapid_move_xy(pts[0].0, pts[0].1));
        seq.extend(cmd_set_speed(speed_mm_s));
        for &(x, y) in &pts[1..] {
            seq.extend(cmd_rapid_move_xy(x, y));
        }
        seq.extend(cmd_set_speed(0.0));
        seq.extend(cmd_rapid_move_xy(cx, cy));
        t.send(&seq).map_err(to_driver_err)
    }

    /// Baut den Job mit Standardparametern (Absolut/Mitte).
    pub fn build_job(&self, plan: &JobPlan) -> Vec<u8> {
        self.build_job_with(plan, &JobParams::default())
    }

    /// Baut den vollständigen, ungeswizzelten Job (Preamble … Trailer) mit
    /// „Starten von"/Anker. Bei `StartMode != Absolut` wird die Geometrie so
    /// verschoben, dass der Ankerpunkt auf (0,0) liegt; die Koordinaten sind dann
    /// (teils negativ) relativ zum Bezugspunkt, den der Controller anwendet.
    pub fn build_job_with(&self, plan: &JobPlan, params: &JobParams) -> Vec<u8> {
        let mut j = Vec::new();

        let bbox = plan.bbox.unwrap_or((0.0, 0.0, 0.0, 0.0));
        // Anker-Offset (µm): nur bei nicht-absolutem Start verschieben.
        let (ox, oy) = if params.start_mode == StartMode::Absolut {
            (0, 0)
        } else {
            let (ax, ay) = params.anchor.point(bbox);
            (-mm_to_um(ax), -mm_to_um(ay))
        };

        let (minx, miny, maxx, maxy) = bbox;
        let minx_um = mm_to_um(minx) + ox;
        let miny_um = mm_to_um(miny) + oy;
        let maxx_um = mm_to_um(maxx) + ox;
        let maxy_um = mm_to_um(maxy) + oy;
        let max_idx = plan.layers.len().saturating_sub(1) as u8;

        // 1. Preamble (Startmodus-Byte + verschobene BBox)
        j.extend(compile_preamble(
            params.start_mode,
            minx_um,
            miny_um,
            maxx_um,
            maxy_um,
        ));
        // 2. Layer-Config
        j.extend(compile_layer_config(&plan.layers, max_idx));
        // 3. F-Block + zweiter BBox-Satz. Ohne diese Register wendet der
        //    Controller den Startmodus aus der Preamble NICHT an — Jobs mit
        //    „Aktuelle Position"/„Benutzerursprung" fuhren dann absolut
        //    (an HW beobachtet; Struktur wie die verifizierte Referenz).
        j.extend(compile_f_block_and_bbox(
            minx_um,
            miny_um,
            maxx_um,
            maxy_um,
            maxx_um - minx_um,
            maxy_um - miny_um,
        ));
        // 4. Geometrie (pro Layer Settings-Block + Bahnen), um den Anker verschoben
        j.extend(self.compile_geometry(&plan.layers, (ox, oy)));
        // 5. Trailer + Dateisumme
        j.extend_from_slice(&[0xEB, 0xE7, 0x00]);
        j.extend_from_slice(&[0xDA, 0x01, 0x06, 0x20]);
        j.extend(encode_coord(minx_um));
        j.extend(encode_coord(miny_um));
        let sum = recompute_file_sum(&j);
        j.extend(sum);

        j
    }

    fn compile_geometry(&self, layers: &[JobLayer], offset: (i32, i32)) -> Vec<u8> {
        let (ox, oy) = offset;
        let mut j = Vec::new();
        for (k, jl) in layers.iter().enumerate() {
            let idx = k as u8;
            if k > 0 {
                j.extend_from_slice(&[0xE7, 0x00]); // Layer-Separator (Folge-Layer)
            }
            write_settings_block(&mut j, jl, idx, k == 0);

            // `passes` (Wiederholungen) wird byte-transparent in die Geometrie
            // gebrannt: der Settings-Block steht einmal, die Fahrwege dahinter
            // werden n-mal wiederholt. Nur so fährt der Controller die Kontur
            // tatsächlich mehrfach — ein einzelner Sende-Vorgang kennt `passes`
            // nicht (Symptom vorher: egal welcher Wert, immer nur 1 Durchlauf).
            let passes = jl.passes.max(1);
            match &jl.work {
                LayerWork::Cut { paths } => {
                    for _ in 0..passes {
                        for path in paths {
                            if path.points.is_empty() {
                                continue;
                            }
                            let (x0, y0) = path.points[0];
                            j.extend(cmd_move_abs(mm_to_um(x0) + ox, mm_to_um(y0) + oy));
                            for &(x, y) in &path.points[1..] {
                                j.extend(cmd_cut_abs(mm_to_um(x) + ox, mm_to_um(y) + oy));
                            }
                            if path.closed {
                                j.extend(cmd_cut_abs(mm_to_um(x0) + ox, mm_to_um(y0) + oy));
                            }
                        }
                    }
                }
                // Fill und Raster fahren beide bidirektionale Scanlinien —
                // derselbe Boustrophedon-Pfad, nur unterschiedliche Herkunft der
                // An-Strecken (Scanline-Segmente bzw. Bild-Runs).
                LayerWork::Fill { .. } | LayerWork::Raster { .. } => {
                    for _ in 0..passes {
                        self.compile_scan(&mut j, jl, offset);
                    }
                }
            }
        }
        j
    }

    /// Scan-Arbeit (Fill wie Bild-Raster) als bidirektionales Raster
    /// (Boustrophedon): Zeilen abwechselnd vor- und rückwärts. Auf die
    /// Rückwärts-Zeilen wirkt der geschwindigkeitsabhängige Scan-Offset
    /// (Vorwärts +off, Rückwärts −off), der den Reversal-Versatz der Röhre
    /// kompensiert. Bei `bidirectional = false` fährt jede Zeile links→rechts;
    /// dann greift kein Offset.
    fn compile_scan(&self, j: &mut Vec<u8>, jl: &JobLayer, offset: (i32, i32)) {
        let (ox, oy) = offset;
        // Nur bei bidirektionalem Scan korrigieren; interpoliert zur Layer-Speed.
        let off = if jl.bidirectional {
            self.config.scan_offset.offset_um(jl.speed_mm_s)
        } else {
            0
        };

        // An-Strecken nach Zeile (y) gruppieren, Zeilen von oben nach unten.
        // Anker-Offset (ox, oy) wird auf jede Koordinate addiert. Fill liefert
        // Scanline-Segmente, Raster die Bild-Runs — beide werden zu (lo, hi).
        let mut by_y: std::collections::BTreeMap<i32, Vec<(i32, i32)>> =
            std::collections::BTreeMap::new();
        let mut add = |y_mm: f64, x0_mm: f64, x1_mm: f64| {
            let y = mm_to_um(y_mm) + oy;
            let (lo, hi) = (mm_to_um(x0_mm) + ox, mm_to_um(x1_mm) + ox);
            let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            by_y.entry(y).or_default().push((lo, hi));
        };
        match &jl.work {
            LayerWork::Fill { segments } => {
                for seg in segments {
                    add(seg.y, seg.x0, seg.x1);
                }
            }
            LayerWork::Raster { rows, .. } => {
                for row in rows {
                    for &(x0, x1) in &row.runs {
                        add(row.y, x0, x1);
                    }
                }
            }
            LayerWork::Cut { .. } => return,
        }

        let mut left_to_right = true;
        // Von oben (größtes y) nach unten.
        for (&y, segs) in by_y.iter().rev() {
            let mut segs = segs.clone();
            segs.sort_by_key(|s| s.0);
            if left_to_right {
                for (lo, hi) in segs {
                    j.extend(cmd_move_abs(lo + off, y));
                    j.extend(cmd_cut_abs(hi + off, y));
                }
            } else {
                for (lo, hi) in segs.into_iter().rev() {
                    j.extend(cmd_move_abs(hi - off, y));
                    j.extend(cmd_cut_abs(lo - off, y));
                }
            }
            if jl.bidirectional {
                left_to_right = !left_to_right;
            }
        }
    }
}

impl MachineDriver for RuidaDriver {
    fn name(&self) -> &str {
        "Ruida"
    }

    fn compile_with(
        &self,
        plan: &JobPlan,
        _layers: &[Layer],
        params: &JobParams,
    ) -> Result<Vec<u8>, String> {
        if plan.is_empty() {
            return Err("Leerer Job — nichts zu lasern.".into());
        }
        // ROHE Job-Bytes (ungeswizzelt, ohne Paket-Checksum) — das ist auch das
        // .rd-Dateiformat. Das Swizzeln + die Paket-Checksum passieren erst beim
        // Senden PRO Chunk (transport.send), weil jedes UDP-Paket seine eigene
        // Checksum braucht. Den ganzen Job in EIN Paket zu packen und dann blind
        // zu chunken war falsch (Chunk 2+ ohne gültige Checksum → Controller
        // verwirft → kein ACK/Timeout; Symptom: „mehr als 1 Layer/Fill fährt nicht").
        Ok(self.build_job_with(plan, params))
    }

    fn connect(&mut self, target: &str) -> Result<(), DriverError> {
        // Idempotent: schon zum selben Ziel verbunden → NICHTS tun. Sonst würde
        // ein zweiter connect() erneut Port 40200 binden (und pingen), was mit
        // dem noch offenen Socket kollidiert → Timeout (Symptom: „fährt einmal,
        // dann keine Antwort"). Bei Zielwechsel neu verbinden.
        if self
            .transport
            .as_ref()
            .is_some_and(|t| t.target_ip() == target)
        {
            return Ok(());
        }
        self.transport = None; // alten Socket freigeben, bevor 40200 neu gebunden wird
        let t = RuidaTransport::connect(target).map_err(to_driver_err)?;
        self.transport = Some(t);
        Ok(())
    }

    fn disconnect(&mut self) {
        self.transport = None;
    }

    fn status(&self) -> Result<MachineStatus, DriverError> {
        let t = self.transport()?;
        let st = read_reg(t, ADDR_STATUS)?;
        let x = read_reg(t, ADDR_POS_X)?;
        let y = read_reg(t, ADDR_POS_Y)?;
        Ok(MachineStatus {
            is_running: st & 0x01 != 0,
            is_paused: st & 0x02 != 0,
            pos_x_mm: x as f64 / 1000.0,
            pos_y_mm: y as f64 / 1000.0,
        })
    }

    fn jog(&self, dx_mm: f64, dy_mm: f64, speed_mm_s: f64) -> Result<(), DriverError> {
        let t = self.transport()?;
        // Ruida-Eilgang ist absolut über beide Achsen. Für relatives Jog lesen wir
        // die aktuelle Position und fahren zu Position + delta (Referenz-Logik).
        let cx = read_reg(t, ADDR_POS_X)?;
        let cy = read_reg(t, ADDR_POS_Y)?;
        let tx = cx + mm_to_um(dx_mm);
        let ty = cy + mm_to_um(dy_mm);
        // Speed + Move müssen in EINEM Paket kommen, sonst fährt der Controller nicht.
        let mut payload = cmd_set_speed(speed_mm_s);
        payload.extend(cmd_rapid_move_xy(tx, ty));
        t.send(&payload).map_err(to_driver_err)
    }

    fn home(&self, speed_mm_s: f64) -> Result<(), DriverError> {
        // Absolut zu (0,0) fahren (Referenzfahrt) — NICHT relativ.
        let t = self.transport()?;
        let mut payload = cmd_set_speed(speed_mm_s);
        payload.extend(cmd_rapid_move_xy(0, 0));
        t.send(&payload).map_err(to_driver_err)
    }

    fn frame(
        &self,
        plan: &JobPlan,
        speed_mm_s: f64,
        params: &JobParams,
    ) -> Result<(), DriverError> {
        let (minx, miny, maxx, maxy) = plan.bbox.ok_or(DriverError::NotSupported)?;
        let pts = vec![
            (mm_to_um(minx), mm_to_um(miny)),
            (mm_to_um(maxx), mm_to_um(miny)),
            (mm_to_um(maxx), mm_to_um(maxy)),
            (mm_to_um(minx), mm_to_um(maxy)),
            (mm_to_um(minx), mm_to_um(miny)),
        ];
        self.drive_frame(pts, speed_mm_s, params)
    }

    fn rubber_frame(
        &self,
        plan: &JobPlan,
        speed_mm_s: f64,
        params: &JobParams,
    ) -> Result<(), DriverError> {
        let hull = plan.convex_hull();
        if hull.len() < 2 {
            return Err(DriverError::NotSupported);
        }
        let mut pts: Vec<(i32, i32)> = hull
            .iter()
            .chain(hull.first())
            .map(|&(x, y)| (mm_to_um(x), mm_to_um(y)))
            .collect();
        // chain(first) schließt den Zug; drive_frame verschiebt und fährt.
        let _ = &mut pts;
        self.drive_frame(pts, speed_mm_s, params)
    }

    fn send_job(&self, bytes: &[u8]) -> Result<(), DriverError> {
        let t = self.transport()?;
        // Stop zuerst befreit den Controller aus einem hängenden Zustand.
        let _ = t.send(&cmd_stop());
        t.drain();
        t.send(bytes).map_err(to_driver_err)
    }

    fn stop(&self) -> Result<(), DriverError> {
        let t = self.transport()?;
        t.send(&cmd_stop()).map_err(to_driver_err)
    }

    fn pause(&self) -> Result<(), DriverError> {
        let t = self.transport()?;
        t.send(&cmd_pause()).map_err(to_driver_err)
    }

    fn read_origin(&self) -> Result<(f64, f64), DriverError> {
        let t = self.transport()?;
        let ox = read_reg(t, ADDR_ORIGIN_X)?;
        let oy = read_reg(t, ADDR_ORIGIN_Y)?;
        Ok((ox as f64 / 1000.0, oy as f64 / 1000.0))
    }

    fn go_origin(&self, speed_mm_s: f64) -> Result<(), DriverError> {
        // Benutzerursprung lesen und absolut dorthin fahren (Referenz-Logik,
        // gotoorigin.pcap) — NICHT die Maschinen-Null (das macht home()).
        let t = self.transport()?;
        let ox = read_reg(t, ADDR_ORIGIN_X)?;
        let oy = read_reg(t, ADDR_ORIGIN_Y)?;
        let mut payload = cmd_set_speed(speed_mm_s);
        payload.extend(cmd_rapid_move_xy(ox, oy));
        t.send(&payload).map_err(to_driver_err)
    }

    fn actions(&self) -> Vec<JobAction> {
        vec![
            JobAction::SendJob,
            JobAction::Frame,
            JobAction::RubberFrame,
            JobAction::Pause,
            JobAction::Home,
            JobAction::GoOrigin,
            JobAction::Stop,
            JobAction::ExportFile,
        ]
    }

    fn run_action(
        &self,
        action: JobAction,
        plan: &JobPlan,
        layers: &[Layer],
        params: &JobParams,
    ) -> Result<String, DriverError> {
        match action {
            JobAction::SendJob => {
                let bytes = self
                    .compile_with(plan, layers, params)
                    .map_err(DriverError::Transport)?;
                self.send_job(&bytes)?;
                Ok(format!("Job gesendet ({} Byte).", bytes.len()))
            }
            JobAction::ExportFile => {
                // Der Aufrufer schreibt die Bytes in eine Datei — hier nur bauen.
                let bytes = self
                    .compile_with(plan, layers, params)
                    .map_err(DriverError::Transport)?;
                Ok(format!("Job kompiliert ({} Byte).", bytes.len()))
            }
            JobAction::Frame => {
                self.frame(plan, 100.0, params)?;
                Ok("Rahmen wird abgefahren.".into())
            }
            JobAction::RubberFrame => {
                self.rubber_frame(plan, 100.0, params)?;
                Ok("Gummiband wird abgefahren.".into())
            }
            JobAction::Pause => {
                self.pause()?;
                Ok("Pause umgeschaltet.".into())
            }
            JobAction::Home => {
                self.home(100.0)?;
                Ok("Referenzfahrt (0/0).".into())
            }
            JobAction::GoOrigin => {
                self.go_origin(100.0)?;
                Ok("Fahre zum Benutzerursprung.".into())
            }
            JobAction::Stop => {
                self.stop()?;
                Ok("Gestoppt.".into())
            }
            JobAction::StreamGcode => Err(DriverError::NotSupported),
        }
    }
}

fn to_driver_err(e: TransportError) -> DriverError {
    DriverError::Transport(e.to_string())
}

/// Register lesen und als Wert (µm bzw. Status-Bits) dekodieren.
fn read_reg(t: &RuidaTransport, addr: u16) -> Result<i32, DriverError> {
    let resp = t.query(&cmd_read_reg(addr)).map_err(to_driver_err)?;
    if resp.len() >= 9 && resp[0] == 0xDA && resp[1] == 0x01 {
        Ok(decode_coord(&resp[4..9]))
    } else {
        Err(DriverError::Transport("unerwartete Antwort".into()))
    }
}

/// Verschiebt Rahmenpunkte (µm) so, dass der `anchor`-Punkt ihrer BBox auf
/// `reference` liegt; `None` (Absolut) lässt sie unverändert. Reine Geometrie —
/// ohne Transport testbar.
fn shift_frame_points(pts: &mut [(i32, i32)], reference: Option<(i32, i32)>, anchor: Anchor) {
    let Some((rx, ry)) = reference else {
        return;
    };
    let (Some(&(x0, y0)), true) = (pts.first(), !pts.is_empty()) else {
        return;
    };
    let mut bbox = (x0, y0, x0, y0);
    for &(x, y) in pts.iter() {
        bbox.0 = bbox.0.min(x);
        bbox.1 = bbox.1.min(y);
        bbox.2 = bbox.2.max(x);
        bbox.3 = bbox.3.max(y);
    }
    // Anchor::point ist linear — funktioniert in µm wie in mm.
    let (ax, ay) = anchor.point((bbox.0 as f64, bbox.1 as f64, bbox.2 as f64, bbox.3 as f64));
    let (dx, dy) = (rx - ax.round() as i32, ry - ay.round() as i32);
    for p in pts.iter_mut() {
        p.0 += dx;
        p.1 += dy;
    }
}

// --- Job-Bausteine (HW-verifiziert, nach Referenz) --------------------------

/// Preamble: Startmodus-Byte, Rahmen-BBox und Diverses. Das Startmodus-Byte sitzt
/// in der Preamble (an HW-Captures verifiziert): Absolut → `D8 10 E6 01 F0`,
/// AktuellePosition → `D8 12 F0`, Benutzerursprung → `D8 11 F0`.
fn compile_preamble(start_mode: StartMode, minx: i32, miny: i32, maxx: i32, maxy: i32) -> Vec<u8> {
    let mut j = Vec::new();
    match start_mode {
        StartMode::Absolut => j.extend_from_slice(&[0xD8, 0x10, 0xE6, 0x01, 0xF0]),
        StartMode::AktuellePosition => j.extend_from_slice(&[0xD8, 0x12, 0xF0]),
        StartMode::Benutzerursprung => j.extend_from_slice(&[0xD8, 0x11, 0xF0]),
    }
    j.extend_from_slice(&[0xF1, 0x02, 0x00]);
    j.extend_from_slice(&[0xD8, 0x00]);
    j.extend_from_slice(&[0xE7, 0x06]);
    j.extend_from_slice(&[0x00; 10]);
    j.extend_from_slice(&[0xE7, 0x38, 0x00]);
    j.extend_from_slice(&[0xE7, 0x03]);
    j.extend(encode_coord(minx));
    j.extend(encode_coord(miny));
    j.extend_from_slice(&[0xE7, 0x07]);
    j.extend(encode_coord(maxx));
    j.extend(encode_coord(maxy));
    j.extend_from_slice(&[0xE7, 0x50]);
    j.extend(encode_coord(minx));
    j.extend(encode_coord(miny));
    j.extend_from_slice(&[0xE7, 0x51]);
    j.extend(encode_coord(maxx));
    j.extend(encode_coord(maxy));
    j.extend_from_slice(&[0xE7, 0x04, 0x00, 0x01, 0x00, 0x01]);
    j.extend_from_slice(&[0x00; 10]);
    j.extend_from_slice(&[0xE7, 0x05, 0x00]);
    j
}

/// F-Block und zweiter BBox-Satz zwischen Layer-Config und Geometrie. Die
/// F1/F2-Register und der E7-13/17/23/37-Satz tragen die (bei relativem
/// Startmodus verschobene) Job-BBox samt Breite/Höhe — aus ihnen leitet der
/// Controller die Job-Platzierung ab. Fehlen sie, ignoriert er das
/// Startmodus-Byte der Preamble und fährt absolut.
fn compile_f_block_and_bbox(minx: i32, miny: i32, maxx: i32, maxy: i32, w: i32, h: i32) -> Vec<u8> {
    let mut j = Vec::new();
    // F-Block.
    j.extend_from_slice(&[0xF1, 0x03]);
    j.extend_from_slice(&[0x00; 10]);
    j.extend_from_slice(&[0xF1, 0x00, 0x00]);
    j.extend_from_slice(&[0xF1, 0x01, 0x00]);
    j.extend_from_slice(&[0xF2, 0x00, 0x00]);
    j.extend_from_slice(&[0xF2, 0x01, 0x00]);
    j.extend_from_slice(&[0xF2, 0x02]);
    j.extend_from_slice(&[0x00; 10]);
    j.extend_from_slice(&[0xF2, 0x03]);
    j.extend(encode_coord(minx));
    j.extend(encode_coord(miny));
    j.extend_from_slice(&[0xF2, 0x04]);
    j.extend(encode_coord(maxx));
    j.extend(encode_coord(maxy));
    j.extend_from_slice(&[0xF2, 0x05, 0x00, 0x01, 0x00, 0x01]);
    j.extend(encode_coord(w));
    j.extend(encode_coord(h));
    j.extend_from_slice(&[0xF2, 0x06]);
    j.extend_from_slice(&[0x00; 10]);
    j.extend_from_slice(&[0xF2, 0x07, 0x00]);
    j.extend_from_slice(&[0xEA, 0x00]);
    // Zweiter BBox-Satz.
    j.extend_from_slice(&[0xE7, 0x60, 0x00]);
    j.extend_from_slice(&[0xE7, 0x13]);
    j.extend(encode_coord(minx));
    j.extend(encode_coord(miny));
    j.extend_from_slice(&[0xE7, 0x17]);
    j.extend(encode_coord(maxx));
    j.extend(encode_coord(maxy));
    j.extend_from_slice(&[0xE7, 0x23]);
    j.extend(encode_coord(minx));
    j.extend(encode_coord(miny));
    j.extend_from_slice(&[0xE7, 0x24, 0x00]);
    j.extend_from_slice(&[0xE7, 0x37]);
    j.extend(encode_coord(maxx));
    j.extend(encode_coord(maxy));
    j.extend_from_slice(&[0xE7, 0x08, 0x00, 0x01, 0x00, 0x01]);
    j.extend(encode_coord(w));
    j.extend(encode_coord(h));
    j
}

/// Layer-Config: pro Layer Speed/Power/Farbe/BBox, dann Abschluss-Blöcke. Die
/// Layer-BBox bleibt in Tischkoordinaten (unverschoben) — so schreibt es die
/// HW-verifizierte Referenz auch bei relativem Startmodus; verschoben wird nur
/// die Geometrie und die Job-BBox in Preamble/F-Block.
fn compile_layer_config(layers: &[JobLayer], max_idx: u8) -> Vec<u8> {
    let mut j = Vec::new();
    for (k, jl) in layers.iter().enumerate() {
        let l = k as u8;
        let (lx0, ly0, lx1, ly1) = jl.bbox().unwrap_or((0.0, 0.0, 0.0, 0.0));
        let (x0, y0) = (mm_to_um(lx0), mm_to_um(ly0));
        let (x1, y1) = (mm_to_um(lx1), mm_to_um(ly1));
        j.extend_from_slice(&[0xC9, 0x04, l]);
        j.extend(encode_speed(jl.speed_mm_s));
        j.extend_from_slice(&[0xC6, 0x31, l]);
        j.extend(encode_power(jl.min_power_pct));
        j.extend_from_slice(&[0xC6, 0x32, l]);
        j.extend(encode_power(jl.power_pct));
        j.extend_from_slice(&[0xC6, 0x41, l]);
        j.extend(encode_power(jl.min_power_pct));
        j.extend_from_slice(&[0xC6, 0x42, l]);
        j.extend(encode_power(jl.power_pct));
        let [r, g, b] = jl.color;
        let bgr = ((b as u64) << 16) | ((g as u64) << 8) | (r as u64);
        j.extend_from_slice(&[0xCA, 0x06, l]);
        j.extend(encode_value(bgr, 5));
        j.extend_from_slice(&[0xCA, 0x41, l, 0x00]);
        j.extend_from_slice(&[0xE7, 0x52, l]);
        j.extend(encode_coord(x0));
        j.extend(encode_coord(y0));
        j.extend_from_slice(&[0xE7, 0x53, l]);
        j.extend(encode_coord(x1));
        j.extend(encode_coord(y1));
        j.extend_from_slice(&[0xE7, 0x61, l]);
        j.extend(encode_coord(x0));
        j.extend(encode_coord(y0));
        j.extend_from_slice(&[0xE7, 0x62, l]);
        j.extend(encode_coord(x1));
        j.extend(encode_coord(y1));
    }
    j.extend_from_slice(&[0xCA, 0x22, max_idx]);
    for code in [0x54u8, 0x55] {
        for l in 0..=max_idx {
            j.extend_from_slice(&[0xE7, code, l]);
            j.extend_from_slice(&[0x00; 5]);
        }
    }
    j
}

/// Settings-Block einer Ebene vor ihrer Geometrie (Layer-Select, Speed, Power).
/// Byte-für-Byte nach der HW-verifizierten Referenz (`_layer_markers`, commit
/// 5982765, Cut+Fill real gefahren) — Reihenfolge/Felder NICHT ohne HW-Test ändern.
fn write_settings_block(j: &mut Vec<u8>, jl: &JobLayer, l: u8, first: bool) {
    // Scan-Arbeit (Fill wie Bild-Raster) gatet das Laser-Feuern auf die X-Fahrt.
    let is_scan = matches!(jl.work, LayerWork::Fill { .. } | LayerWork::Raster { .. });
    // CA 01 01 = Raster-Gating (Laser feuert nur bei X-Fahrt) — nur beim ERSTEN
    // Layer und nur für Fill/Image; sonst CA 01 00. (Referenz-verifiziert.)
    let gating = if first && is_scan { 0x01 } else { 0x00 };
    j.extend_from_slice(&[0xCA, 0x01, gating]);
    j.extend_from_slice(&[0xCA, 0x02, l]);
    j.extend_from_slice(&[0xCA, 0x01, 0x30]);
    j.extend_from_slice(&[0xCA, 0x01, 0x10]);
    // Luftunterstützung: CA 01 13 = an, CA 01 12 = aus (pro Layer).
    j.extend_from_slice(&[0xCA, 0x01, if jl.air_assist { 0x13 } else { 0x12 }]);
    j.extend_from_slice(&[0xC9, 0x02]);
    j.extend(encode_speed(jl.speed_mm_s));
    // Aktive Power-Felder C6 01/02/21/22 (min/max, doppelte Quelle).
    let pw_min = encode_power(jl.min_power_pct);
    let pw_max = encode_power(jl.power_pct);
    j.extend_from_slice(&[0xC6, 0x01]);
    j.extend(pw_min.clone());
    j.extend_from_slice(&[0xC6, 0x02]);
    j.extend(pw_max.clone());
    j.extend_from_slice(&[0xC6, 0x21]);
    j.extend(pw_min);
    j.extend_from_slice(&[0xC6, 0x22]);
    j.extend(pw_max);
    // Laser-2-Korrektur = 0 (in der Referenz immer vorhanden, hatte gefehlt).
    j.extend_from_slice(&[0xC6, 0x12]);
    j.extend_from_slice(&[0x00; 5]);
    j.extend_from_slice(&[0xC6, 0x13]);
    j.extend_from_slice(&[0x00; 5]);
    // Nur Cut: C6 50/51 = Power 0.
    if !is_scan {
        let pw0 = encode_power(0.0);
        j.extend_from_slice(&[0xC6, 0x50]);
        j.extend(pw0.clone());
        j.extend_from_slice(&[0xC6, 0x51]);
        j.extend(pw0);
    }
    j.extend_from_slice(&[0xCA, 0x03, 0x01]); // Layer-Block-Start
}

#[cfg(test)]
mod tests {
    use super::*;
    use luxifer_core::{Anchor, AppState, Geo};

    fn plan_one_rect() -> JobPlan {
        let mut st = AppState::new();
        st.add_shape(Geo::Rect {
            x: 1.0,
            y: 2.0,
            w: 10.0,
            h: 5.0,
        });
        JobPlan::from_shapes(&st.shapes, &st.layers)
    }

    /// Baut aus einem Raster-Plan Bytes und prüft das Scan-Gating. Der Plan wird
    /// im Test von Hand gebaut (ohne `image`-Dependency im Treiber); die echte
    /// Bild→Raster-Kette testet der Core.
    #[test]
    fn raster_plan_setzt_scan_gating() {
        use luxifer_core::{JobLayer, LayerWork, RasterRow};
        let plan = JobPlan {
            layers: vec![JobLayer {
                layer_id: 0,
                color: [0, 0, 0],
                speed_mm_s: 100.0,
                power_pct: 50.0,
                min_power_pct: 10.0,
                passes: 1,
                air_assist: false,
                bidirectional: true,
                work: LayerWork::Raster {
                    rows: vec![RasterRow {
                        y: 1.0,
                        runs: vec![(0.0, 4.0)],
                    }],
                    texture: None,
                },
            }],
            bbox: Some((0.0, 1.0, 4.0, 1.0)),
        };
        let job = RuidaDriver::default().build_job(&plan);
        assert!(
            job.windows(3).any(|w| w == [0xCA, 0x01, 0x01]),
            "Raster muss (wie Fill) das Scan-Gating setzen"
        );
        assert!(
            job.contains(&0x88) && job.contains(&0xA8),
            "move+cut erwartet"
        );
        assert_eq!(*job.last().unwrap(), END_OF_FILE);
    }

    #[test]
    fn job_beginnt_mit_preamble_und_endet_mit_eof() {
        let plan = plan_one_rect();
        let job = RuidaDriver::default().build_job(&plan);
        assert_eq!(&job[..2], &[0xD8, 0x10]); // Startmodus Absolut
        assert_eq!(*job.last().unwrap(), END_OF_FILE); // 0xD7
    }

    #[test]
    fn startmodus_setzt_preamble_byte() {
        let plan = plan_one_rect();
        let d = RuidaDriver::default();
        // Absolut → D8 10; AktuellePosition → D8 12; Benutzerursprung → D8 11.
        let abs = d.build_job_with(&plan, &JobParams::default());
        assert_eq!(&abs[..2], &[0xD8, 0x10]);
        let akt = d.build_job_with(
            &plan,
            &JobParams {
                start_mode: StartMode::AktuellePosition,
                ..Default::default()
            },
        );
        assert_eq!(&akt[..2], &[0xD8, 0x12]);
        let usr = d.build_job_with(
            &plan,
            &JobParams {
                start_mode: StartMode::Benutzerursprung,
                ..Default::default()
            },
        );
        assert_eq!(&usr[..2], &[0xD8, 0x11]);
    }

    /// Sucht eine Bytefolge im Job.
    fn contains_seq(job: &[u8], seq: &[u8]) -> bool {
        job.windows(seq.len()).any(|w| w == seq)
    }

    #[test]
    fn rahmenpunkte_verschieben_anker_auf_referenz() {
        // Rechteck-Rahmen (0,0)–(10,20) mm in µm; Anker Mitte → (5,10) mm.
        // Referenz (Kopfposition) bei (100, 50) mm: alle Punkte verschieben
        // sich um (95, 40) mm. Absolut (None) lässt sie unverändert.
        let mut pts = vec![(0, 0), (10_000, 0), (10_000, 20_000), (0, 20_000), (0, 0)];
        let original = pts.clone();
        shift_frame_points(&mut pts, None, Anchor::Center);
        assert_eq!(pts, original, "Absolut verschiebt nicht");

        shift_frame_points(&mut pts, Some((100_000, 50_000)), Anchor::Center);
        assert_eq!(pts[0], (95_000, 40_000));
        assert_eq!(pts[2], (105_000, 60_000));

        // Anker NW: die Min-Ecke landet direkt auf der Referenz.
        let mut pts = original.clone();
        shift_frame_points(&mut pts, Some((100_000, 50_000)), Anchor::NW);
        assert_eq!(pts[0], (100_000, 50_000));
    }

    #[test]
    fn job_enthaelt_f_block_und_zweiten_bbox_satz() {
        // Regression: Ohne F-Block/zweiten BBox-Satz ignorierte der Controller
        // das Startmodus-Byte — „Aktuelle Position"/„Benutzerursprung" fuhren
        // absolut. Struktur wie die HW-verifizierte Referenz.
        let plan = plan_one_rect();
        let job = RuidaDriver::default().build_job(&plan);
        for marker in [
            &[0xF1u8, 0x03][..],
            &[0xF2, 0x03],
            &[0xF2, 0x04],
            &[0xE7, 0x60, 0x00],
            &[0xE7, 0x13],
            &[0xE7, 0x37],
            &[0xE7, 0x08, 0x00, 0x01, 0x00, 0x01],
        ] {
            assert!(contains_seq(&job, marker), "Marker {marker:02X?} fehlt");
        }
    }

    #[test]
    fn relativer_startmodus_verschiebt_job_bbox_nicht_layer_bbox() {
        // Rechteck bei (1,2)–(11,7) mm, Anker = Mitte (Index 4): der Job-Nullpunkt
        // liegt auf der BBox-Mitte (6, 4.5) mm → verschobene Job-BBox beginnt bei
        // (-5000, -2500) µm. Die Layer-BBox (E7 52) bleibt in Tischkoordinaten.
        let plan = plan_one_rect();
        let job = RuidaDriver::default().build_job_with(
            &plan,
            &JobParams {
                start_mode: StartMode::AktuellePosition,
                anchor: Anchor::Center,
            },
        );
        // F2 03 trägt die verschobene Job-BBox-Ecke (-5 mm, -2.5 mm).
        let mut expected = vec![0xF2, 0x03];
        expected.extend(encode_coord(-5_000));
        expected.extend(encode_coord(-2_500));
        assert!(
            contains_seq(&job, &expected),
            "verschobene Job-BBox im F-Block"
        );
        // E7 52 (Layer 0) trägt die UNVERSCHOBENE Layer-BBox-Ecke (1 mm, 2 mm).
        let mut layer_bbox = vec![0xE7, 0x52, 0x00];
        layer_bbox.extend(encode_coord(1_000));
        layer_bbox.extend(encode_coord(2_000));
        assert!(
            contains_seq(&job, &layer_bbox),
            "Layer-BBox in Tischkoordinaten"
        );
    }

    #[test]
    fn job_enthaelt_layer_config_und_geometrie() {
        let plan = plan_one_rect();
        let job = RuidaDriver::default().build_job(&plan);
        // Layer-Config: Speed-Opcode C9 04.
        assert!(job.windows(2).any(|w| w == [0xC9, 0x04]));
        // Geometrie: move (88) und cut (A8).
        assert!(job.contains(&0x88));
        assert!(job.contains(&0xA8));
    }

    #[test]
    fn compile_liefert_rohe_jobbytes() {
        // compile() gibt die ROHEN Job-Bytes (= .rd-Format, ungeswizzelt, ohne
        // Paket-Checksum). Swizzle + Checksum passieren erst beim Senden PRO Chunk
        // (transport). So bekommt jedes UDP-Paket seine eigene gültige Checksum.
        let plan = plan_one_rect();
        let compiled = RuidaDriver::default().compile(&plan, &[]).unwrap();
        let raw = RuidaDriver::default().build_job(&plan);
        assert_eq!(compiled, raw, "compile == roher Job (kein Paket-Wrapper)");
        assert_eq!(*compiled.last().unwrap(), END_OF_FILE);
    }

    #[test]
    fn leerer_job_ist_fehler() {
        let plan = JobPlan {
            layers: vec![],
            bbox: None,
        };
        assert!(RuidaDriver::default().compile(&plan, &[]).is_err());
    }

    #[test]
    fn passes_wiederholen_die_geometrie() {
        // passes wird byte-transparent in die Geometrie gebrannt (ADR 0006 §7):
        // 3 Durchläufe erzeugen einen längeren Job als 1 — die Kontur steckt
        // n-fach in den Bytes, der Controller fährt sie so tatsächlich mehrfach.
        let mut st = AppState::new();
        st.add_shape(Geo::Rect {
            x: 1.0,
            y: 2.0,
            w: 10.0,
            h: 5.0,
        });
        let plan1 = JobPlan::from_shapes(&st.shapes, &st.layers);
        st.layers[0].passes = 3;
        let plan3 = JobPlan::from_shapes(&st.shapes, &st.layers);
        let job1 = RuidaDriver::default().build_job(&plan1);
        let job3 = RuidaDriver::default().build_job(&plan3);
        assert!(
            job3.len() > job1.len(),
            "3 Passes müssen mehr Bytes ergeben als 1"
        );
    }

    #[test]
    fn scan_offset_verschiebt_fill_zeilen() {
        // Fill-Layer, bidirektional. Mit aktivem Scan-Offset unterscheidet sich
        // der Job von dem ohne Offset (verschobene Zeilen), aber gleich lang.
        let mut st = AppState::new();
        st.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 5.0,
        });
        st.layers[0].mode = luxifer_core::LayerMode::Fill;
        st.layers[0].bidirectional = true;
        let plan = JobPlan::from_shapes(&st.shapes, &st.layers);

        let ohne = RuidaDriver::default().build_job(&plan);
        let mit = RuidaDriver::new(RuidaConfig {
            scan_offset: ScanOffset::from_linear(0.001, 100.0),
        })
        .build_job(&plan);
        assert_ne!(ohne, mit, "aktiver Scan-Offset muss den Job verändern");
        assert_eq!(
            ohne.len(),
            mit.len(),
            "nur Werte verschoben, gleiche Byte-Zahl"
        );
    }

    #[test]
    fn settings_block_enthaelt_hw_verifizierte_felder() {
        // Cut-Layer: C6 12/13 (Laser-2-Korrektur) UND C6 50/51 vorhanden;
        // kein Raster-Gating (CA 01 01) für Cut. Referenz-verifiziert.
        let mut st = AppState::new();
        st.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        let plan = JobPlan::from_shapes(&st.shapes, &st.layers);
        let cut = RuidaDriver::default().build_job(&plan);
        assert!(cut.windows(2).any(|w| w == [0xC6, 0x12]), "C6 12 fehlt");
        assert!(cut.windows(2).any(|w| w == [0xC6, 0x13]), "C6 13 fehlt");
        assert!(
            cut.windows(2).any(|w| w == [0xC6, 0x50]),
            "C6 50 (Cut) fehlt"
        );
        assert!(
            cut.windows(2).any(|w| w == [0xC6, 0x51]),
            "C6 51 (Cut) fehlt"
        );
        assert!(
            !cut.windows(3).any(|w| w == [0xCA, 0x01, 0x01]),
            "Cut darf kein Raster-Gating CA 01 01 haben"
        );

        // Fill-Layer als erster: Raster-Gating CA 01 01 vorhanden, keine C6 50/51.
        let mut st2 = AppState::new();
        st2.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        st2.layers[0].mode = luxifer_core::LayerMode::Fill;
        let plan2 = JobPlan::from_shapes(&st2.shapes, &st2.layers);
        let fill = RuidaDriver::default().build_job(&plan2);
        assert!(
            fill.windows(3).any(|w| w == [0xCA, 0x01, 0x01]),
            "Fill (erster Layer) braucht Raster-Gating CA 01 01"
        );
        assert!(
            !fill.windows(2).any(|w| w == [0xC6, 0x50]),
            "Fill darf kein C6 50 haben"
        );
    }
}
