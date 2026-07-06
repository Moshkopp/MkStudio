//! Geräteunabhängige Job-Repräsentation (`JobPlan`) und der `MachineDriver`-Trait.
//!
//! Der Core wandelt Shapes + Layer in einen `JobPlan` (Bewegungen in mm, nach
//! Layer gruppiert). Konkrete Treiber (Ruida, GRBL, miniGRBL) übersetzen den
//! Plan in ihr Format — der Core kennt selbst KEIN Gerät (ADR 0001).

use crate::geometry::{rotate_point, Geo, Pt};
use crate::model::{Layer, LayerMode, Shape};
use crate::scanline::{fill_segments, Contour, FillSegment};

/// Ein zusammenhängender Pfad in mm (Polygonzug). `closed` = Kontur schließt sich.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub points: Vec<Pt>,
    pub closed: bool,
}

/// Die auf einem Layer auszuführende Arbeit (geräteunabhängig).
#[derive(Debug, Clone, PartialEq)]
pub enum LayerWork {
    /// Konturen abfahren (Cut/Gravur-Linien).
    Cut { paths: Vec<Path> },
    /// Fläche mit horizontalen Linien füllen (Even-Odd-Scanline).
    Fill { segments: Vec<FillSegment> },
    // Raster { ... } folgt mit dem Bild-Support.
}

/// Ein Layer-Block des Jobs: Parameter + Arbeit. Referenziert den Original-Layer.
#[derive(Debug, Clone, PartialEq)]
pub struct JobLayer {
    pub layer_id: usize,
    /// Layerfarbe (manche Treiber, z. B. Ruida, kodieren sie in der Config).
    pub color: [u8; 3],
    pub speed_mm_s: f64,
    pub power_pct: f64,
    pub min_power_pct: f64,
    pub passes: u32,
    pub work: LayerWork,
}

impl JobLayer {
    /// Bounding-Box der Arbeit dieses Layers in mm (min_x, min_y, max_x, max_y).
    pub fn bbox(&self) -> Option<(f64, f64, f64, f64)> {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut any = false;
        let mut acc = |x: f64, y: f64| {
            any = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        };
        match &self.work {
            LayerWork::Cut { paths } => {
                for p in paths {
                    for &(x, y) in &p.points {
                        acc(x, y);
                    }
                }
            }
            LayerWork::Fill { segments } => {
                for s in segments {
                    acc(s.x0, s.y);
                    acc(s.x1, s.y);
                }
            }
        }
        any.then_some((min_x, min_y, max_x, max_y))
    }
}

/// Der komplette, geräteunabhängige Job. Alle Maße in mm.
#[derive(Debug, Clone, PartialEq)]
pub struct JobPlan {
    pub layers: Vec<JobLayer>,
    /// Bounding-Box aller Geometrie (mm): (min_x, min_y, max_x, max_y).
    pub bbox: Option<(f64, f64, f64, f64)>,
}

impl JobPlan {
    /// Baut den Plan aus Shapes und Layern. Nur **aktive, nicht gesperrte**
    /// Layer kommen hinein; unsichtbare werden übersprungen. Rotation wird auf
    /// die Punkte angewandt, sodass Treiber nur noch fertige mm-Pfade sehen.
    ///
    /// Cut-Layer erhalten Kontur-Pfade, Fill-Layer eine Scanline-Füllung
    /// (Zeilenabstand aus `line_step_mm`). Raster wird bis zum Bild-Support wie
    /// Fill behandelt. Nur **aktive, nicht gesperrte, sichtbare** Layer.
    pub fn from_shapes(shapes: &[Shape], layers: &[Layer]) -> JobPlan {
        let mut job_layers: Vec<JobLayer> = Vec::new();

        for (li, layer) in layers.iter().enumerate() {
            // Nur aktivierte Layer werden gebrannt. `visible` steuert nur die
            // Canvas-Anzeige, nicht den Job (ADR/Model: enabled ≠ visible).
            if !layer.enabled {
                continue;
            }
            let paths: Vec<Path> = shapes
                .iter()
                .filter(|s| s.layer_id == li)
                .map(shape_to_path)
                .collect();
            if paths.is_empty() {
                continue;
            }

            let work = if layer.mode.is_filled() {
                // Fill/Raster: geschlossene Konturen des Layers gemeinsam füllen.
                let contours: Vec<Contour> = paths
                    .iter()
                    .map(|p| Contour {
                        points: &p.points,
                        closed: p.closed,
                    })
                    .collect();
                let segments = fill_segments(&contours, layer.line_step_mm);
                LayerWork::Fill { segments }
            } else {
                LayerWork::Cut { paths }
            };

            job_layers.push(JobLayer {
                layer_id: li,
                color: layer.color,
                speed_mm_s: layer.speed_mm_s,
                power_pct: layer.power_pct,
                min_power_pct: layer.min_power_pct,
                passes: layer.passes,
                work,
            });
        }

        let bbox = bounding_box(&job_layers);
        JobPlan {
            layers: job_layers,
            bbox,
        }
    }

    /// Ob der Plan überhaupt Arbeit enthält.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

/// Wandelt eine Shape (inkl. Rotation) in einen mm-Pfad.
fn shape_to_path(s: &Shape) -> Path {
    let (mut points, closed) = raw_points(&s.geo);
    if s.rotation != 0.0 {
        let (cx, cy) = s.bbox().center();
        for p in points.iter_mut() {
            *p = rotate_point(p.0, p.1, cx, cy, s.rotation);
        }
    }
    Path { points, closed }
}

/// Rohe Punkte einer Geometrie (ohne Rotation) + ob geschlossen.
fn raw_points(geo: &Geo) -> (Vec<Pt>, bool) {
    match geo {
        Geo::Rect { x, y, w, h } => (
            vec![(*x, *y), (*x + *w, *y), (*x + *w, *y + *h), (*x, *y + *h)],
            true,
        ),
        Geo::Ellipse { cx, cy, rx, ry } => {
            let segs = 64;
            let mut pts = Vec::with_capacity(segs);
            for i in 0..segs {
                let a = (i as f64 / segs as f64) * std::f64::consts::TAU;
                pts.push((cx + rx * a.cos(), cy + ry * a.sin()));
            }
            (pts, true)
        }
        Geo::Polyline { pts, closed } => (pts.clone(), *closed),
    }
}

fn bounding_box(layers: &[JobLayer]) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut any = false;
    let mut acc = |x: f64, y: f64| {
        any = true;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };
    for jl in layers {
        match &jl.work {
            LayerWork::Cut { paths } => {
                for p in paths {
                    for &(x, y) in &p.points {
                        acc(x, y);
                    }
                }
            }
            LayerWork::Fill { segments } => {
                for s in segments {
                    acc(s.x0, s.y);
                    acc(s.x1, s.y);
                }
            }
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

/// Was ein Treiber können muss. Der Core kennt keine Gerätedetails; die GUI
/// spricht ausschließlich über diesen Trait (ADR 0001).
///
/// `compile` ist der Kern: geräteunabhängiger Plan → gerätespezifische Bytes
/// (Ruida-Binär bzw. G-Code als UTF-8). Die Live-Steuerung (jog/home/…) kommt,
/// sobald der erste Treiber gebaut wird.
pub trait MachineDriver {
    /// Name des Treibers (z. B. "Ruida", "GRBL").
    fn name(&self) -> &str;

    /// Übersetzt den Plan in gerätespezifische Job-Daten.
    fn compile(&self, plan: &JobPlan, layers: &[Layer]) -> Result<Vec<u8>, String>;
}

/// Ob ein Layer-Modus (perspektivisch) Flächenfüllung braucht.
/// Für die spätere Fill/Raster-Erweiterung.
pub fn needs_fill(mode: LayerMode) -> bool {
    mode.is_filled()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn state_one_rect() -> AppState {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        });
        s
    }

    #[test]
    fn rect_wird_geschlossener_pfad_mit_vier_punkten() {
        let s = state_one_rect();
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert_eq!(plan.layers.len(), 1);
        let LayerWork::Cut { paths } = &plan.layers[0].work else {
            panic!("Cut erwartet")
        };
        assert_eq!(paths.len(), 1);
        assert!(paths[0].closed);
        assert_eq!(paths[0].points.len(), 4);
        assert_eq!(paths[0].points[0], (10.0, 20.0));
    }

    #[test]
    fn bbox_umschliesst_geometrie() {
        let s = state_one_rect();
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert_eq!(plan.bbox, Some((10.0, 20.0, 40.0, 60.0)));
    }

    #[test]
    fn deaktivierter_layer_wird_uebersprungen() {
        let mut s = state_one_rect();
        s.layers[0].enabled = false;
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert!(plan.is_empty());
    }

    #[test]
    fn unsichtbarer_aber_aktivierter_layer_wird_gebrannt() {
        // visible steuert nur die Anzeige, nicht den Job.
        let mut s = state_one_rect();
        s.layers[0].visible = false;
        s.layers[0].enabled = true;
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert!(!plan.is_empty(), "unsichtbarer aktivierter Layer brennt");
    }

    #[test]
    fn rotation_wird_auf_punkte_angewandt() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 20.0,
        });
        s.shapes[0].rotation = 90.0;
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        let LayerWork::Cut { paths } = &plan.layers[0].work else {
            panic!("Cut erwartet")
        };
        // Um 90° gedreht muss die Bounding-Box der Punkte ~20 breit, ~100 hoch sein.
        let xs: Vec<f64> = paths[0].points.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = paths[0].points.iter().map(|p| p.1).collect();
        let w = xs.iter().cloned().fold(f64::MIN, f64::max)
            - xs.iter().cloned().fold(f64::MAX, f64::min);
        let h = ys.iter().cloned().fold(f64::MIN, f64::max)
            - ys.iter().cloned().fold(f64::MAX, f64::min);
        assert!((w - 20.0).abs() < 1e-6, "Breite nach Drehung ~20, war {w}");
        assert!((h - 100.0).abs() < 1e-6, "Höhe nach Drehung ~100, war {h}");
    }

    #[test]
    fn ellipse_wird_polygonisiert() {
        let mut s = AppState::new();
        s.add_shape(Geo::Ellipse {
            cx: 0.0,
            cy: 0.0,
            rx: 10.0,
            ry: 5.0,
        });
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        let LayerWork::Cut { paths } = &plan.layers[0].work else {
            panic!("Cut erwartet")
        };
        assert!(paths[0].closed);
        assert!(paths[0].points.len() >= 32);
    }
}
