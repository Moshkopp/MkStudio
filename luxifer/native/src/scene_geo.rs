//! Wandelt den Core-`AppState` in Zeichendaten (Linien-Vertices) für den
//! wgpu-Canvas. Reines Zeichnen — keine Fachlogik. Farben kommen aus den Layern,
//! Rotation wird wie im Core (`rotate_point`) angewendet.

use luxifer_core::geometry::{rotate_point, Pt};
use luxifer_core::state::AppState;

/// Ein Vertex für dicke Linien im Welt-Raum (mm). `pos` = Punkt auf der
/// Segment-Mittellinie, `dir` = normierte Segmentrichtung (Welt), `side` = -1/+1
/// (welche Seite des Quads). Der Vertex-Shader verschiebt um `side` senkrecht zu
/// `dir` um die halbe Linienbreite in PIXELN — so ist die Dicke zoom-unabhängig
/// konstant und die Geometrie bleibt cache-fähig (Kamera nur im Shader).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub dir: [f32; 2],
    pub side: f32,
    pub color: [f32; 4],
}

fn col(rgb: [u8; 3], a: f32) -> [f32; 4] {
    [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
        a,
    ]
}

/// Weltpunkte einer Form inkl. Rotation (um den BBox-Mittelpunkt), wie der Core
/// sie für Hit-Test/BBox verwendet. Auch die Projektbrowser-Miniatur nutzt das,
/// damit es genau EINE Outline-Ableitung gibt.
pub(crate) fn world_outline(shape: &luxifer_core::model::Shape) -> (Vec<(f64, f64)>, bool) {
    let (pts, closed) = shape.geo.outline_points();
    if shape.rotation.abs() <= f64::EPSILON {
        return (pts, closed);
    }
    let c = shape.geo.bbox().center();
    let rot = pts
        .into_iter()
        .map(|(x, y)| rotate_point(x, y, c.0, c.1, shape.rotation))
        .collect();
    (rot, closed)
}

/// Baut die Linien-Vertices (LineList) für alle sichtbaren Shapes. Selektierte
/// Shapes bekommen die Akzentfarbe, sonst die Layer-Farbe.
/// Konturen aller sichtbaren Shapes in Layer-Farbe. Bewusst OHNE Auswahl-
/// Einfärbung: Die Akzentuierung selektierter Shapes liegt im Overlay
/// (`selected_outlines`), damit dieser gecachte Puffer nur an der Geometrie
/// hängt und nicht bei jeder Auswahländerung neu gebaut werden muss.
pub fn shape_lines(state: &AppState) -> Vec<Vertex> {
    let mut v = Vec::new();
    for (index, shape) in state.shapes.iter().enumerate() {
        if state.selected.contains(&index) {
            continue;
        }
        let layer = state.layers.get(shape.layer_id);
        let visible = layer.map(|l| l.visible).unwrap_or(true);
        let fill_mode = layer.map(|l| l.mode.is_filled()).unwrap_or(false);
        if !visible || (shape.fill_only && !fill_mode) {
            continue;
        }
        let base = layer.map(|l| l.color).unwrap_or([200, 200, 200]);
        let (pts, closed) = world_outline(shape);
        push_polyline(&mut v, &pts, closed, col(base, 1.0));
    }
    v
}

/// Konturen der aktuell selektierten Shapes in Akzentfarbe — jeden Frame im
/// Overlay über die gecachten Konturen gezeichnet. Nur die Auswahl, daher
/// billig; unsichtbare Layer bleiben ausgespart.
pub fn selected_outlines(state: &AppState, accent: [u8; 3]) -> Vec<Vertex> {
    let mut v = Vec::new();
    for &i in &state.selected {
        let Some(shape) = state.shapes.get(i) else {
            continue;
        };
        let visible = state
            .layers
            .get(shape.layer_id)
            .map(|l| l.visible)
            .unwrap_or(true);
        let fill_mode = state
            .layers
            .get(shape.layer_id)
            .map(|l| l.mode.is_filled())
            .unwrap_or(false);
        if !visible || (shape.fill_only && !fill_mode) {
            continue;
        }
        let (pts, closed) = world_outline(shape);
        push_polyline(&mut v, &pts, closed, col(accent, 1.0));
    }
    v
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillCompoundBatch {
    pub stencil: std::ops::Range<u32>,
    pub cover: std::ops::Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillBatch {
    pub compounds: Vec<FillCompoundBatch>,
    pub cover: std::ops::Range<u32>,
}

/// Normale Design-Flächenfüllung. Jede geschlossene Kontur wird als
/// Dreiecksfächer in den GPU-Stencil geschrieben. Das Paritätsbit bildet über
/// alle Konturen eines Layers dieselbe Even-Odd-Semantik wie der Laser-Fill,
/// ohne dessen Scanlinien im Design-Canvas zu erzeugen oder zu zeigen.
pub fn solid_fills(state: &AppState) -> (Vec<Vertex>, Vec<FillBatch>) {
    let mut vertices = Vec::new();
    let mut batches = Vec::new();
    for (li, layer) in state.layers.iter().enumerate() {
        // Image-Shapes werden von `image_gpu` als echte Texturen gezeichnet.
        // Ihre rechteckige Outline darf hier nicht mit Scanlines in der
        // Bild-Layer-Kennfarbe über die Textur gemalt werden.
        if !layer.visible || !layer.mode.is_filled() || layer.mode == luxifer_core::LayerMode::Image
        {
            continue;
        }
        let mut compounds: Vec<(Option<u32>, Vec<Vec<Pt>>)> = Vec::new();
        for shape in state.shapes.iter().filter(|shape| shape.layer_id == li) {
            let (points, closed) = world_outline(shape);
            if !closed || points.len() < 3 {
                continue;
            }
            let fill_group_id = shape.fill_group_id.or(shape.group_id);
            if let Some(id) = fill_group_id {
                if let Some((_, rings)) = compounds
                    .iter_mut()
                    .find(|(candidate, _)| *candidate == Some(id))
                {
                    rings.push(points);
                    continue;
                }
            }
            compounds.push((fill_group_id, vec![points]));
        }
        if compounds.is_empty() {
            continue;
        }

        let mut bounds = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for points in compounds.iter().flat_map(|(_, rings)| rings) {
            for point in points {
                bounds.0 = bounds.0.min(point.0);
                bounds.1 = bounds.1.min(point.1);
                bounds.2 = bounds.2.max(point.0);
                bounds.3 = bounds.3.max(point.1);
            }
        }

        let mut compound_batches = Vec::with_capacity(compounds.len());
        for (_, rings) in &compounds {
            let stencil_start = vertices.len() as u32;
            let mut compound_bounds = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for points in rings {
                for point in points {
                    compound_bounds.0 = compound_bounds.0.min(point.0);
                    compound_bounds.1 = compound_bounds.1.min(point.1);
                    compound_bounds.2 = compound_bounds.2.max(point.0);
                    compound_bounds.3 = compound_bounds.3.max(point.1);
                }
                let anchor = points[0];
                for triangle in points[1..].windows(2) {
                    for point in [anchor, triangle[0], triangle[1]] {
                        vertices.push(raw_vertex(point, [0.0; 4]));
                    }
                }
            }
            let stencil_end = vertices.len() as u32;
            let compound_cover = stencil_end;
            push_solid_rect(
                &mut vertices,
                compound_bounds.0,
                compound_bounds.1,
                compound_bounds.2,
                compound_bounds.3,
                [0.0; 4],
            );
            compound_batches.push(FillCompoundBatch {
                stencil: stencil_start..stencil_end,
                cover: compound_cover..vertices.len() as u32,
            });
        }
        let cover_start = vertices.len() as u32;
        push_solid_rect(
            &mut vertices,
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            col(layer.color, 0.32),
        );
        batches.push(FillBatch {
            compounds: compound_batches,
            cover: cover_start..vertices.len() as u32,
        });
    }
    (vertices, batches)
}

fn raw_vertex(point: Pt, color: [f32; 4]) -> Vertex {
    Vertex {
        pos: [point.0 as f32, point.1 as f32],
        dir: [0.0, 0.0],
        side: 0.0,
        color,
    }
}

pub(crate) fn push_solid_rect(
    vertices: &mut Vec<Vertex>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    color: [f32; 4],
) {
    for point in [(x0, y0), (x1, y0), (x1, y1), (x0, y0), (x1, y1), (x0, y1)] {
        vertices.push(raw_vertex(point, color));
    }
}

/// Ein Liniensegment als dickes Quad (2 Dreiecke = 6 Vertices). Die Dicke trägt
/// der Shader im Screen-Space auf (siehe `Vertex`).
pub fn push_seg(v: &mut Vec<Vertex>, a: [f32; 2], b: [f32; 2], color: [f32; 4]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return;
    }
    let dir = [dx / len, dy / len];
    let mk = |pos: [f32; 2], side: f32| Vertex {
        pos,
        dir,
        side,
        color,
    };
    // Zwei Dreiecke: (a-, a+, b-) und (a+, b+, b-).
    v.push(mk(a, -1.0));
    v.push(mk(a, 1.0));
    v.push(mk(b, -1.0));
    v.push(mk(a, 1.0));
    v.push(mk(b, 1.0));
    v.push(mk(b, -1.0));
}

fn push_polyline(v: &mut Vec<Vertex>, pts: &[(f64, f64)], closed: bool, color: [f32; 4]) {
    if pts.len() < 2 {
        return;
    }
    for w in pts.windows(2) {
        push_seg(
            v,
            [w[0].0 as f32, w[0].1 as f32],
            [w[1].0 as f32, w[1].1 as f32],
            color,
        );
    }
    if closed {
        let (a, b) = (pts[pts.len() - 1], pts[0]);
        push_seg(v, [a.0 as f32, a.1 as f32], [b.0 as f32, b.1 as f32], color);
    }
}

/// Rechteck-Umriss (Welt) als dicke Linien — für Tisch-Rahmen und Auswahl-BBox.
pub fn rect_outline(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Vec<Vertex> {
    let p = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
    let mut v = Vec::new();
    for i in 0..4 {
        push_seg(&mut v, p[i], p[(i + 1) % 4], color);
    }
    v
}

/// Kleines gefülltes Quadrat um einen Weltpunkt (Halbkante `hw` mm) für
/// Transform-Handles: als dicke Rahmen-Linien + Diagonalen (wirkt solide).
pub fn handle_marker(cx: f32, cy: f32, hw: f32, color: [f32; 4]) -> Vec<Vertex> {
    let (l, r, t, b) = (cx - hw, cx + hw, cy - hw, cy + hw);
    let corners = [[l, t], [r, t], [r, b], [l, b]];
    let mut v = Vec::new();
    for i in 0..4 {
        push_seg(&mut v, corners[i], corners[(i + 1) % 4], color);
    }
    push_seg(&mut v, [l, t], [r, b], color);
    push_seg(&mut v, [r, t], [l, b], color);
    v
}

/// Ruhiger Transform-Griff ohne X-Diagonalen. Ecken bleiben quadratisch,
/// Seiten werden als kurze Balken dargestellt und kommunizieren so ihre Achse.
pub fn transform_handle(
    cx: f32,
    cy: f32,
    half_w: f32,
    half_h: f32,
    color: [f32; 4],
) -> Vec<Vertex> {
    let mut v = fill_rect(
        cx - half_w,
        cy - half_h,
        half_w * 2.0,
        half_h * 2.0,
        [0.035, 0.045, 0.06, 1.0],
    );
    v.extend(rect_outline(
        cx - half_w,
        cy - half_h,
        half_w * 2.0,
        half_h * 2.0,
        color,
    ));
    v
}

/// Eigenständiger runder Rotationsgriff mit Mittelpunkt statt Resize-X.
pub fn rotation_handle(cx: f32, cy: f32, radius: f32, color: [f32; 4]) -> Vec<Vertex> {
    let mut v = Vec::new();
    const SEGMENTS: usize = 20;
    let mut previous = [cx + radius, cy];
    for i in 1..=SEGMENTS {
        let angle = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let point = [cx + radius * angle.cos(), cy + radius * angle.sin()];
        push_seg(&mut v, previous, point, color);
        previous = point;
    }
    v.extend(fill_rect(
        cx - radius * 0.22,
        cy - radius * 0.22,
        radius * 0.44,
        radius * 0.44,
        color,
    ));
    v
}

/// Baut das Arbeitsbett: Rahmen + mm-Gitter (grob alle `major` mm kräftiger,
/// fein alle `minor` mm dezent) + Nullpunkt-Kreuz. Gibt dem Canvas das Gefühl
/// einer Werkbank statt leeren Graus.
/// Gefülltes Rechteck (2 Dreiecke) mit side=0 (keine Verdickung) — für Flächen.
pub fn fill_rect(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Vec<Vertex> {
    let mk = |px: f32, py: f32| Vertex {
        pos: [px, py],
        dir: [1.0, 0.0],
        side: 0.0,
        color,
    };
    vec![
        mk(x, y),
        mk(x + w, y),
        mk(x + w, y + h),
        mk(x, y),
        mk(x + w, y + h),
        mk(x, y + h),
    ]
}

/// Arbeitsbereich als Rahmen + Nullpunkt-Kreuz. Die Fläche bleibt transparent,
/// damit das viewportfüllende Gitter innen und außen gleichmäßig lesbar ist.
pub fn bed_base(w: f32, h: f32, origin: luxifer_core::BedOrigin) -> Vec<Vertex> {
    let mut v = bed_outline(w, h);
    push_origin_marker(&mut v, origin, (w as f64, h as f64));
    v
}

/// Klarer Maschinen-Nullmarker bei (0,0): verlängertes L innerhalb des Betts,
/// Kreuz über beide Achsen und eine kleine Raute im Schnittpunkt. Der Marker
/// bezeichnet bewusst nicht den controllerseitigen Benutzerursprung.
fn push_origin_marker(v: &mut Vec<Vertex>, origin: luxifer_core::BedOrigin, bed: (f64, f64)) {
    let first = v.len();
    let arm = 24.0;
    let cross = 8.0;
    let diamond = 4.0_f64;
    let point = |x: f64, y: f64| {
        let p = origin.transform(x, y, bed);
        [p.0 as f32, p.1 as f32]
    };
    push_seg(v, point(0.0, 0.0), point(arm, 0.0), ORIGIN_COLOR);
    push_seg(v, point(0.0, 0.0), point(0.0, arm), ORIGIN_COLOR);
    push_seg(v, point(-cross, 0.0), point(cross, 0.0), ORIGIN_COLOR);
    push_seg(v, point(0.0, -cross), point(0.0, cross), ORIGIN_COLOR);
    let diamond_points: Vec<_> = [
        (0.0, -diamond),
        (diamond, 0.0),
        (0.0, diamond),
        (-diamond, 0.0),
    ]
    .into_iter()
    .map(|(x, y)| origin.transform(x, y, bed))
    .collect();
    push_polyline(v, &diamond_points, true, ORIGIN_COLOR);
    for vertex in &mut v[first..] {
        vertex.side *= 1.8;
    }
}

/// Deutlich hervorgehobener Bett-Rahmen. `side` ist im Shader der
/// Multiplikator der bildschirmkonstanten Linienbreite; der Rahmen kann damit
/// kräftiger sein, ohne alle Objektkonturen ebenfalls zu verbreitern.
fn bed_outline(w: f32, h: f32) -> Vec<Vertex> {
    let mut frame = rect_outline(0.0, 0.0, w, h, BED_COLOR);
    for vertex in &mut frame {
        vertex.side *= 1.8;
    }
    frame
}

/// Unterhalb dieses Bildschirm-Abstands (px pro Rasterschritt) ist eine
/// Linienschar komplett ausgeblendet — dort begänne Moiré/Flimmern.
pub const GRID_FADE_LO_PX: f32 = 7.0;
/// Ab diesem Abstand hat die Feinlinien-Schar ihre volle Deckkraft.
pub const GRID_FADE_HI_PX: f32 = 14.0;

/// Wählt die Feinraster-Schrittweite für den aktuellen Zoom: die Settings-
/// Rasterweite, bei Bedarf ×5 vergröbert, bis ein Schritt auf dem Bildschirm
/// mindestens [`GRID_FADE_LO_PX`] misst. ×5 hält die Progression konsistent
/// zur Hauptlinien-Regel „jede 5. Linie": Die bisherige Hauptlinie wird beim
/// Rauszoomen nahtlos zur neuen Feinlinie.
pub fn grid_step_mm(grid_mm: f32, scale_px_per_mm: f32) -> f32 {
    let mut step = grid_mm.max(0.05);
    while step * scale_px_per_mm < GRID_FADE_LO_PX {
        step *= 5.0;
    }
    step
}

/// Gitter über den gesamten sichtbaren Ausschnitt (H1): am Maschinen-Nullpunkt
/// ausgerichtete Linien, Hauptlinien alle 5 Schritte. Zoom-adaptiv gegen
/// Moiré: Die Feinlinien blenden zwischen [`GRID_FADE_HI_PX`] und
/// [`GRID_FADE_LO_PX`] Bildschirm-Abstand weich aus; parallel sinkt die
/// Hauptlinien-Deckkraft auf Feinlinien-Niveau, sodass die Hauptlinie beim
/// Stufenwechsel (×5) ohne Sprung zur Feinlinie der nächsten Stufe wird.
pub fn viewport_grid(cam: &crate::camera::Camera, grid_mm: f32) -> Vec<Vertex> {
    let step = grid_step_mm(grid_mm, cam.scale);
    let t = ((step * cam.scale - GRID_FADE_LO_PX) / (GRID_FADE_HI_PX - GRID_FADE_LO_PX))
        .clamp(0.0, 1.0);
    const FINE_A: f32 = 0.018;
    const COARSE_A: f32 = 0.055;
    let fine = [1.0, 1.0, 1.0, FINE_A * t];
    let coarse = [1.0, 1.0, 1.0, FINE_A + (COARSE_A - FINE_A) * t];

    // Sichtbarer Weltausschnitt (mm), einen Schritt großzügiger.
    let tl = cam.screen_to_world([0.0, 0.0]);
    let br = cam.screen_to_world(cam.viewport);
    let (x0, x1) = (tl[0] as f32 - step, br[0] as f32 + step);
    let (y0, y1) = (tl[1] as f32 - step, br[1] as f32 + step);

    // Linien über den Schritt-Index statt Modulo — bei krummen Rasterweiten
    // driftet Float-Modulo und Hauptlinien würden zufällig „fein".
    let mut v = Vec::new();
    let mut axis = |a0: f32, a1: f32, vertical: bool| {
        let i0 = (a0 / step).floor() as i64;
        let i1 = (a1 / step).ceil() as i64;
        for i in i0..=i1 {
            let color = if i.rem_euclid(5) == 0 { coarse } else { fine };
            if color[3] < 0.005 {
                continue;
            }
            let a = i as f32 * step;
            if vertical {
                push_seg(&mut v, [a, y0], [a, y1], color);
            } else {
                push_seg(&mut v, [x0, a], [x1, a], color);
            }
        }
    };
    axis(x0, x1, true);
    axis(y0, y1, false);
    // Das Raster ist Orientierung, nicht Inhalt: schmaler als Objektkonturen
    // zeichnen, obwohl beide denselben bildschirmkonstanten Shader verwenden.
    for vertex in &mut v {
        vertex.side *= 0.55;
    }
    v
}

/// Materialfläche für die Laser-Vorschau: Untergrund in Materialfarbe mit
/// dezentem Rahmen — ohne Gitter und Nullpunkt-Kreuz, das Material ist die
/// Bühne, nicht der Messtisch.
pub fn bed_material(w: f32, h: f32, color: [f32; 4]) -> Vec<Vertex> {
    let mut v = fill_rect(0.0, 0.0, w, h, color);
    for seg in bed_outline(w, h) {
        v.push(seg);
    }
    v
}

/// Farbwert für den deutlich sichtbaren Tisch-Rahmen.
pub const BED_COLOR: [f32; 4] = [0.62, 0.65, 0.7, 0.95];
/// Auswahl-BBox-Rahmen (heller Akzentton).
pub const SEL_BOX_COLOR: [f32; 4] = [0.4, 0.7, 1.0, 0.9];
/// Fenster-Auswahl rechts→links: nur vollständig enthalten.
pub const MARQUEE_CONTAIN_COLOR: [f32; 4] = [0.18, 0.9, 0.42, 0.9];
/// Kreuz-Auswahl links→rechts: Berührung genügt.
pub const MARQUEE_CROSS_COLOR: [f32; 4] = [0.95, 0.24, 0.22, 0.9];
/// Transform-Handles (weiß).
pub const HANDLE_COLOR: [f32; 4] = [0.95, 0.97, 1.0, 1.0];
/// Maschinen-Nullmarker (leuchtendes Akzentgrün).
pub const ORIGIN_COLOR: [f32; 4] = [0.2, 0.95, 0.62, 1.0];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bild_layer_wird_nicht_mit_seiner_kennfarbe_uebermalt() {
        let mut state = AppState::new();
        state.add_image("asset".into(), 0.0, 0.0, 20.0, 10.0);

        let (vertices, batches) = solid_fills(&state);
        assert!(vertices.is_empty());
        assert!(batches.is_empty());
    }

    #[test]
    fn design_fill_ist_feste_flaeche_und_unabhaengig_vom_laser_zeilenabstand() {
        let mut state = AppState::new();
        state.add_shape(luxifer_core::Geo::Rect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        });
        state.layers[0].mode = luxifer_core::LayerMode::Fill;
        state.layers[0].line_step_mm = 0.1;
        let fine = solid_fills(&state);
        state.layers[0].line_step_mm = 10.0;
        let coarse = solid_fills(&state);

        assert_eq!(fine, coarse);
        assert_eq!(fine.1.len(), 1);
        assert_eq!(fine.1[0].compounds.len(), 1);
        assert_eq!(fine.1[0].compounds[0].stencil, 0..6);
        assert_eq!(fine.1[0].compounds[0].cover, 6..12);
        assert_eq!(fine.1[0].cover, 12..18);
    }

    #[test]
    fn offene_kontur_erzeugt_keine_design_flaeche() {
        let mut state = AppState::new();
        state.add_shape(luxifer_core::Geo::Polyline {
            pts: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)],
            closed: false,
        });
        state.layers[0].mode = luxifer_core::LayerMode::Fill;

        let (vertices, batches) = solid_fills(&state);
        assert!(vertices.is_empty());
        assert!(batches.is_empty());
    }

    #[test]
    fn getrennte_shapes_sind_getrennte_fill_compounds() {
        let mut state = AppState::new();
        state.add_shape(luxifer_core::Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        state.add_shape(luxifer_core::Geo::Rect {
            x: 5.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        state.layers[0].mode = luxifer_core::LayerMode::Fill;

        let (_, batches) = solid_fills(&state);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].compounds.len(), 2);
    }

    #[test]
    fn stress_1808_getrennte_fill_compounds_bleiben_linear_aufbaubar() {
        let mut state = AppState::new();
        for index in 0..1_808 {
            let column = (index % 64) as f64;
            let row = (index / 64) as f64;
            state.add_shape(luxifer_core::Geo::Rect {
                x: column * 2.0,
                y: row * 2.0,
                w: 1.0,
                h: 1.0,
            });
        }
        state.layers[0].mode = luxifer_core::LayerMode::Fill;

        let started = std::time::Instant::now();
        let (vertices, batches) = solid_fills(&state);
        let elapsed = started.elapsed();

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].compounds.len(), 1_808);
        assert_eq!(vertices.len(), 1_808 * 12 + 6);
        let fill_draw_calls = batches[0].compounds.len() * 3 + 2;
        assert_eq!(fill_draw_calls, 5_426);
        eprintln!(
            "fill_stress compounds=1808 vertices={} cpu_build_ms={:.3} fill_draw_calls={fill_draw_calls}",
            vertices.len(),
            elapsed.as_secs_f64() * 1_000.0,
        );
    }

    /// Beim Arbeitszoom bleibt der Schritt exakt die Settings-Rasterweite;
    /// erst wenn ein Schritt unter die Moiré-Schwelle fiele, vergröbert ×5.
    #[test]
    fn gitter_schritt_vergroebert_erst_unter_der_moire_schwelle() {
        // 10 mm × 2 px/mm = 20 px ≥ 7 px → Settings-Raster unverändert.
        assert_eq!(grid_step_mm(10.0, 2.0), 10.0);
        // 10 mm × 0.5 px/mm = 5 px < 7 px → eine Stufe ×5 (50 mm = 25 px).
        assert_eq!(grid_step_mm(10.0, 0.5), 50.0);
        // Sehr weit raus: mehrere Stufen, aber immer 5er-Potenz des Rasters.
        let step = grid_step_mm(10.0, 0.02);
        assert!(step * 0.02 >= GRID_FADE_LO_PX);
        assert_eq!(step % 10.0, 0.0);
    }

    /// Das viewportfüllende Gitter darf bei keinem Zoom zur Tapete werden:
    /// Der Linienabstand auf dem Schirm bleibt über der Moiré-Schwelle,
    /// also ist die Linienzahl durch die Viewport-Pixel begrenzt.
    #[test]
    fn viewport_gitter_bleibt_bei_jedem_zoom_begrenzt() {
        for scale in [0.02_f32, 0.1, 1.0, 10.0, 2000.0] {
            let cam = crate::camera::Camera {
                center: [450.0, 300.0],
                scale,
                viewport: [1920.0, 1080.0],
            };
            let v = viewport_grid(&cam, 1.0);
            // 6 Vertices pro Segment; je Achse maximal Viewport/Schwelle
            // Linien (+ Überhang) → großzügige Obergrenze.
            let lines = v.len() / 6;
            assert!(lines < 700, "scale {scale}: {lines} Linien");
        }
    }
}
