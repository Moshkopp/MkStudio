//! Kurzlebiger Interaktions- und Kamerazustand des Canvas. Reines UI-Anliegen:
//! welches Werkzeug aktiv ist, welche Geste läuft, wo der Cursor steht, wie die
//! Kamera steht. Die Fach-Wahrheit bleibt im Core (`EditorSession`); dieser
//! Zustand steuert nur Darstellung und Eingabe.

use crate::camera::Camera;
use crate::tools::{Drag, Tool};

/// Schwebender Haltesteg-Entwurf: Linie (Welt-mm) + Steg-Breite. Bleibt nach
/// dem Ziehen stehen (Endpunkte nachfassbar, Breite über das Eingabefeld am
/// Linienende), bis der Nutzer bestätigt oder abbricht.
#[derive(Clone, Copy)]
pub struct BridgeDraft {
    pub p0: [f64; 2],
    pub p1: [f64; 2],
    pub width: f64,
}

pub struct CanvasState {
    pub cam: Camera,
    pub tool: Tool,
    /// Aktive Polygon-Form (beim Polygon-Werkzeug aufgezogen).
    pub active_shape: luxifer_core::PolyShape,
    /// Laufende Maus-Geste (zwischen Press und Release).
    pub drag: Drag,
    /// Cursor in Fensterpixeln (für Welt-Umrechnung).
    pub cursor: [f32; 2],
    /// Wird beim UI-Aufbau gesetzt, damit native Werkzeugcursor nur innerhalb
    /// des eigentlichen Canvas den egui-Cursor ersetzen.
    pub cursor_over_canvas: bool,
    pub space_down: bool,
    pub ctrl_down: bool,
    pub shift_down: bool,
    /// Benutzerpräferenz: Fenster-/Kreuz-Auswahlrichtungen vertauschen.
    pub invert_marquee_direction: bool,
    /// Punkt-Zug (Welt-Punkte), bis Doppelklick/Enter schließt.
    pub poly_pts: Vec<(f64, f64)>,
    /// Schwebender Haltesteg-Entwurf (nur beim Bridge-Werkzeug).
    pub bridge: Option<BridgeDraft>,
    /// Zuletzt genutzte Steg-Breite (mm) — Vorbelegung des nächsten Entwurfs.
    pub bridge_width: f64,
    /// Native Bézier-Feder: Anker samt beim Ziehen erzeugten Tangenten.
    pub bezier_nodes: Vec<luxifer_core::bezier::BezierNode>,
    /// Core-berechneter Abschnitt unter dem Cursor, der beim Trim-Klick entfällt.
    pub trim_preview: Option<Vec<(f64, f64)>>,
    /// Nur im Laser-Tab: Layer, deren Shapes vorübergehend transformierbar sind.
    /// `None` = normale Design-Bearbeitung, `Some` = Laser-Policy aktiv.
    pub laser_editable_layers: Option<std::collections::HashSet<usize>>,
    /// Letzter Links-Klick (Zeit + Weltposition) für die Doppelklick-Erkennung.
    last_click: Option<(std::time::Instant, [f64; 2])>,
}

impl CanvasState {
    pub fn new(cam: Camera) -> Self {
        Self {
            cam,
            tool: Tool::Select,
            active_shape: luxifer_core::PolyShape::Penta,
            drag: Drag::None,
            cursor: [0.0, 0.0],
            cursor_over_canvas: false,
            space_down: false,
            ctrl_down: false,
            shift_down: false,
            invert_marquee_direction: false,
            poly_pts: Vec::new(),
            bridge: None,
            bridge_width: 2.0,
            bezier_nodes: Vec::new(),
            trim_preview: None,
            laser_editable_layers: None,
            last_click: None,
        }
    }

    /// Prüft, ob der Klick an `w` (Welt) ein Doppelklick zum vorherigen ist, und
    /// merkt ihn als neuen „letzten Klick". Doppelklick = innerhalb 400 ms und
    /// nah an der vorigen Position.
    pub(super) fn is_double_click(&mut self, w: [f64; 2]) -> bool {
        let now = std::time::Instant::now();
        let double = self.last_click.is_some_and(|(t, p)| {
            now.duration_since(t).as_millis() < 400
                && (p[0] - w[0]).hypot(p[1] - w[1]) < 5.0 / self.cam.scale as f64
        });
        self.last_click = if double { None } else { Some((now, w)) };
        double
    }

    /// Cursor-Weltkoordinaten (mm).
    pub fn world(&self) -> [f64; 2] {
        self.cam.screen_to_world(self.cursor)
    }
}
