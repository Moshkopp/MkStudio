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
    for shape in &state.shapes {
        let layer = state.layers.get(shape.layer_id);
        let visible = layer.map(|l| l.visible).unwrap_or(true);
        if !visible {
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
        if !visible {
            continue;
        }
        let (pts, closed) = world_outline(shape);
        push_polyline(&mut v, &pts, closed, col(accent, 1.0));
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
        // Image-Shapes werden von `image_gpu` als echte Texturen gezeichnet.
        // Ihre rechteckige Outline darf hier nicht mit Scanlines in der
        // Bild-Layer-Kennfarbe über die Textur gemalt werden.
        if !layer.visible || !layer.mode.is_filled() || layer.mode == luxifer_core::LayerMode::Image
        {
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

/// Arbeitsbereich als Bühne: dezent hellere Fläche + kräftiger Rahmen +
/// Nullpunkt-Kreuz. Das Gitter liegt NICHT mehr hier (zoom-unabhängig
/// gecacht), sondern im kamera-abhängigen [`viewport_grid`].
pub fn bed_base(w: f32, h: f32) -> Vec<Vertex> {
    // Bett-Fläche zuunterst, etwas heller als der Fenster-Hintergrund.
    let mut v = fill_rect(0.0, 0.0, w, h, [0.10, 0.11, 0.13, 1.0]);
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
    const FINE_A: f32 = 0.05;
    const COARSE_A: f32 = 0.12;
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
    v
}

/// Materialfläche für die Laser-Vorschau: Untergrund in Materialfarbe mit
/// dezentem Rahmen — ohne Gitter und Nullpunkt-Kreuz, das Material ist die
/// Bühne, nicht der Messtisch.
pub fn bed_material(w: f32, h: f32, color: [f32; 4]) -> Vec<Vertex> {
    let mut v = fill_rect(0.0, 0.0, w, h, color);
    for seg in rect_outline(0.0, 0.0, w, h, BED_COLOR) {
        v.push(seg);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bild_layer_wird_nicht_mit_seiner_kennfarbe_uebermalt() {
        let mut state = AppState::new();
        state.add_image("asset".into(), 0.0, 0.0, 20.0, 10.0);

        assert!(fill_lines(&state).is_empty());
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
