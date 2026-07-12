//! Wandelt den Core-`AppState` in Zeichendaten (Linien-Vertices) für den
//! wgpu-Canvas. Reines Zeichnen — keine Fachlogik. Farben kommen aus den Layern,
//! Rotation wird wie im Core (`rotate_point`) angewendet.

use luxifer_core::geometry::{rotate_point, Pt};
use luxifer_core::scanline::{fill_segments, Contour};
use luxifer_core::state::AppState;

/// Ein Vertex für dicke Linien im Welt-Raum (mm). `pos` = Punkt auf der
/// Segment-Mittellinie, `dir` = normierte Segmentrichtung (Welt), `side` = -1/+1
/// (welche Seite des Quads). Der Vertex-Shader verschiebt um `side` senkrecht zu
/// `dir` um die halbe Linienbreite in PIXELN — so ist die Dicke zoom-unabhängig
/// konstant und die Geometrie bleibt cache-fähig (Kamera nur im Shader).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
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
/// sie für Hit-Test/BBox verwendet.
fn world_outline(shape: &luxifer_core::model::Shape) -> (Vec<(f64, f64)>, bool) {
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
pub fn shape_lines(state: &AppState, accent: [u8; 3]) -> Vec<Vertex> {
    let mut v = Vec::new();
    for (i, shape) in state.shapes.iter().enumerate() {
        let layer = state.layers.get(shape.layer_id);
        let visible = layer.map(|l| l.visible).unwrap_or(true);
        if !visible {
            continue;
        }
        let selected = state.selected.contains(&i);
        let base = layer.map(|l| l.color).unwrap_or([200, 200, 200]);
        let color = if selected {
            col(accent, 1.0)
        } else {
            col(base, 1.0)
        };

        let (pts, closed) = world_outline(shape);
        push_polyline(&mut v, &pts, closed, color);
    }
    v
}

/// Baut die Füll-Vertices für alle fillbaren, sichtbaren Layer: der Core rechnet
/// die Even-Odd-Scanline-Segmente (`fill_segments`), wir zeichnen sie als
/// horizontale Linien in Layer-Farbe. Das ist der Aztec-Stresstest (73k Segmente)
/// — und derselbe Fill wie in der Laser-Vorschau, kein neuer Algorithmus.
pub fn fill_lines(state: &AppState) -> Vec<Vertex> {
    let mut v = Vec::new();
    for (li, layer) in state.layers.iter().enumerate() {
        if !layer.visible || !layer.mode.is_filled() {
            continue;
        }
        // Alle (rotierten) Welt-Konturen dieses Layers gemeinsam füllen, damit
        // überlappende Formen und Löcher korrekt kombiniert werden.
        let rings: Vec<(Vec<Pt>, bool)> = state
            .shapes
            .iter()
            .filter(|s| s.layer_id == li)
            .map(world_outline)
            .collect();
        let contours: Vec<Contour> = rings
            .iter()
            .map(|(pts, closed)| Contour {
                points: pts,
                closed: *closed,
            })
            .collect();
        if contours.is_empty() {
            continue;
        }
        let step = layer.line_step_mm.max(0.05);
        let color = col(layer.color, 0.9);
        for seg in fill_segments(&contours, step) {
            push_seg(
                &mut v,
                [seg.x0 as f32, seg.y as f32],
                [seg.x1 as f32, seg.y as f32],
                color,
            );
        }
    }
    v
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

pub fn bed_grid(w: f32, h: f32) -> Vec<Vertex> {
    // Bett-Fläche zuunterst, etwas heller als der Fenster-Hintergrund.
    let mut v = fill_rect(0.0, 0.0, w, h, [0.10, 0.11, 0.13, 1.0]);
    let minor = 10.0_f32;
    let major = 50.0_f32;
    let fine = [1.0, 1.0, 1.0, 0.05];
    let coarse = [1.0, 1.0, 1.0, 0.12];

    // Vertikale Linien.
    let mut x = 0.0;
    while x <= w + 0.01 {
        let is_major = (x % major).abs() < 0.01 || ((x % major) - major).abs() < 0.01;
        push_seg(
            &mut v,
            [x, 0.0],
            [x, h],
            if is_major { coarse } else { fine },
        );
        x += minor;
    }
    // Horizontale Linien.
    let mut y = 0.0;
    while y <= h + 0.01 {
        let is_major = (y % major).abs() < 0.01 || ((y % major) - major).abs() < 0.01;
        push_seg(
            &mut v,
            [0.0, y],
            [w, y],
            if is_major { coarse } else { fine },
        );
        y += minor;
    }
    // Bett-Rahmen kräftiger.
    for seg in rect_outline(0.0, 0.0, w, h, BED_COLOR) {
        v.push(seg);
    }
    // Nullpunkt-Kreuz (links oben = Maschinen-0) in Akzentfarbe.
    let o = 14.0;
    push_seg(&mut v, [0.0, 0.0], [o, 0.0], ORIGIN_COLOR);
    push_seg(&mut v, [0.0, 0.0], [0.0, o], ORIGIN_COLOR);
    v
}

/// Farbwert für den Tisch-Rahmen (dezentes Grau).
pub const BED_COLOR: [f32; 4] = [0.42, 0.46, 0.52, 0.9];
/// Auswahl-BBox-Rahmen (heller Akzentton).
pub const SEL_BOX_COLOR: [f32; 4] = [0.4, 0.7, 1.0, 0.9];
/// Transform-Handles (weiß).
pub const HANDLE_COLOR: [f32; 4] = [0.95, 0.97, 1.0, 1.0];
/// Nullpunkt-Kreuz (Akzentgrün).
pub const ORIGIN_COLOR: [f32; 4] = [0.25, 0.72, 0.5, 1.0];
