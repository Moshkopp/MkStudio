//! Overlay-Vertices: Auswahl-BBox, Transform-Handles und die Live-Zeichen-
//! vorschau (Aufzieh-Box, Punkt-Zug-Gummiband). Jeden Frame neu gebaut, aber
//! winzig; kamera-abhängig. Reine Funktion — liest nur den übergebenen Zustand.

use luxifer_application::EditorSession;
use luxifer_core::PolyShape;

use crate::scene_geo::{self, Vertex};
use crate::tools::{Drag, Tool};

/// Eingaben für den Overlay-Aufbau. Bündelt den (nur gelesenen) Interaktions-
/// und Kamerazustand, damit das Overlay ohne `App`-Zugriff auskommt.
pub struct OverlayInput<'a> {
    pub session: &'a EditorSession,
    pub accent: [u8; 3],
    pub drag: &'a Drag,
    pub tool: Tool,
    pub active_shape: PolyShape,
    pub poly_pts: &'a [(f64, f64)],
    /// Weltkoordinaten des Cursors (mm) — vom Root aus Kamera + Cursor bestimmt.
    pub world_cursor: [f64; 2],
    /// Kamera-Skalierung (px/mm) für bildschirmkonstante Markergrößen.
    pub cam_scale: f32,
}

/// Halbe Handle-Kantenlänge in Welt-mm, damit sie am Bildschirm konstant
/// ~7px groß wirken (unabhängig vom Zoom). Auch für den Handle-Hit-Test genutzt.
pub(crate) fn handle_hw(cam_scale: f32) -> f32 {
    7.0 / cam_scale
}

/// Rotate-Handle-Position (mm): mittig über der Auswahl-BBox, mit Abstand.
/// Auch für den Rotate-Handle-Hit-Test genutzt.
pub(crate) fn rotate_handle_pos(b: &luxifer_core::BBox, cam_scale: f32) -> [f64; 2] {
    let off = 22.0 / cam_scale as f64;
    [b.x + b.w / 2.0, b.y - off]
}

/// Zeichnet ein gestricheltes Segment (Welt-mm) als kurze Striche. `scale` =
/// Pixel pro mm, damit die Strichlänge am Bildschirm konstant wirkt.
fn dashed_seg(v: &mut Vec<Vertex>, a: [f32; 2], b: [f32; 2], color: [f32; 4], scale: f32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-4 {
        return;
    }
    let dir = [dx / len, dy / len];
    let dash = 6.0 / scale; // ~6 px Strich
    let gap = 4.0 / scale; // ~4 px Lücke
    let step = dash + gap;
    let mut t = 0.0;
    while t < len {
        let s = [a[0] + dir[0] * t, a[1] + dir[1] * t];
        let e_t = (t + dash).min(len);
        let e = [a[0] + dir[0] * e_t, a[1] + dir[1] * e_t];
        scene_geo::push_seg(v, s, e, color);
        t += step;
    }
}

/// Baut die Overlay-Vertices (Auswahl, Handles, Live-Zeichenvorschau).
pub fn overlay_vertices(input: &OverlayInput) -> Vec<Vertex> {
    let mut v = Vec::new();
    let cur = input.world_cursor;

    // Selektierte Shapes in Akzentfarbe über die (auswahlfreien) gecachten
    // Konturen legen — jeden Frame, damit der Vertex-Cache auswahlfrei bleibt.
    v.extend(scene_geo::selected_outlines(input.session, input.accent));

    let preview = [0.6, 0.8, 1.0, 0.9];
    // Live-Vorschau beim Aufziehen eines Rechtecks/einer Ellipse/Linie.
    if let Drag::DrawBox { start } = input.drag {
        let start = *start;
        match input.tool {
            Tool::Ellipse => {
                let x = start[0].min(cur[0]) as f32;
                let y = start[1].min(cur[1]) as f32;
                let w = (start[0] - cur[0]).abs() as f32;
                let h = (start[1] - cur[1]).abs() as f32;
                let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                let (rx, ry) = (w / 2.0, h / 2.0);
                let n = 48;
                let mut prev = [cx + rx, cy];
                for i in 1..=n {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    let p = [cx + rx * a.cos(), cy + ry * a.sin()];
                    scene_geo::push_seg(&mut v, prev, p, preview);
                    prev = p;
                }
            }
            Tool::Line | Tool::Measure => {
                scene_geo::push_seg(
                    &mut v,
                    [start[0] as f32, start[1] as f32],
                    [cur[0] as f32, cur[1] as f32],
                    preview,
                );
            }
            Tool::Polygon => {
                // Form vom Zentrum aufziehen (Vorschau der gewählten PolyShape).
                let r = (start[0] - cur[0]).hypot(start[1] - cur[1]);
                if r > 0.5 {
                    let pts = input.active_shape.points(start[0], start[1], r, 0.0);
                    if pts.len() >= 2 {
                        for wnd in pts.windows(2) {
                            scene_geo::push_seg(
                                &mut v,
                                [wnd[0].0 as f32, wnd[0].1 as f32],
                                [wnd[1].0 as f32, wnd[1].1 as f32],
                                preview,
                            );
                        }
                        // Schlusskante.
                        let (f, l) = (pts[0], pts[pts.len() - 1]);
                        scene_geo::push_seg(
                            &mut v,
                            [l.0 as f32, l.1 as f32],
                            [f.0 as f32, f.1 as f32],
                            preview,
                        );
                    }
                }
            }
            _ => {
                let x = start[0].min(cur[0]) as f32;
                let y = start[1].min(cur[1]) as f32;
                let w = (start[0] - cur[0]).abs() as f32;
                let h = (start[1] - cur[1]).abs() as f32;
                v.extend(scene_geo::rect_outline(x, y, w, h, preview));
            }
        }
    }

    // Live-Vorschau des Punkt-Zugs (Polyline/Spline/Bézier/Polygon): gesetzte
    // Segmente + gestricheltes Gummiband zur Maus + Punkt-Marker, wie Tauri.
    if !input.poly_pts.is_empty()
        && matches!(input.tool, Tool::Polyline | Tool::Spline | Tool::Bezier)
    {
        let col = [0.9, 0.9, 0.95, 0.9];
        // Gesetzte Segmente.
        for wnd in input.poly_pts.windows(2) {
            scene_geo::push_seg(
                &mut v,
                [wnd[0].0 as f32, wnd[0].1 as f32],
                [wnd[1].0 as f32, wnd[1].1 as f32],
                col,
            );
        }
        // Gummiband vom letzten Punkt zur Maus (gestrichelt).
        let last = *input.poly_pts.last().unwrap();
        dashed_seg(
            &mut v,
            [last.0 as f32, last.1 as f32],
            [cur[0] as f32, cur[1] as f32],
            [1.0, 1.0, 1.0, 0.4],
            input.cam_scale,
        );
        // Punkt-Marker (kleine Quadrate); Startpunkt hervorgehoben.
        let hw = 3.0 / input.cam_scale;
        for (i, p) in input.poly_pts.iter().enumerate() {
            let c = if i == 0 {
                [0.25, 0.72, 0.5, 1.0] // Start grün (Schließen-Signal)
            } else {
                [0.3, 0.51, 0.97, 1.0]
            };
            v.extend(scene_geo::handle_marker(p.0 as f32, p.1 as f32, hw, c));
        }
    }

    // Auswahl-BBox toolunabhängig anzeigen (nicht im gecachten Puffer, damit
    // dieser nicht an der Auswahl hängt).
    if let Some(b) = input.session.selection_bbox() {
        v.extend(scene_geo::rect_outline(
            b.x as f32,
            b.y as f32,
            b.w as f32,
            b.h as f32,
            scene_geo::SEL_BOX_COLOR,
        ));
    }

    // Handles nur im Auswahl-Werkzeug und bei vorhandener Auswahl.
    if input.tool != Tool::Select {
        return v;
    }
    let Some(b) = input.session.selection_bbox() else {
        return v;
    };
    let hw = handle_hw(input.cam_scale);
    for (_, (hx, hy)) in luxifer_core::Handle::positions(&b) {
        v.extend(scene_geo::handle_marker(
            hx as f32,
            hy as f32,
            hw,
            scene_geo::HANDLE_COLOR,
        ));
    }
    // Rotate-Handle: Linie von oben-Mitte nach oben + Kreis-Marker.
    let rp = rotate_handle_pos(&b, input.cam_scale);
    let top = [b.x as f32 + b.w as f32 / 2.0, b.y as f32];
    scene_geo::push_seg(
        &mut v,
        top,
        [rp[0] as f32, rp[1] as f32],
        scene_geo::SEL_BOX_COLOR,
    );
    v.extend(scene_geo::handle_marker(
        rp[0] as f32,
        rp[1] as f32,
        hw * 1.1,
        scene_geo::HANDLE_COLOR,
    ));
    v
}
