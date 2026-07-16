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
    pub bezier_nodes: &'a [luxifer_core::bezier::BezierNode],
    /// Weltkoordinaten des Cursors (mm) — vom Root aus Kamera + Cursor bestimmt.
    pub world_cursor: [f64; 2],
    /// Kamera-Skalierung (px/mm) für bildschirmkonstante Markergrößen.
    pub cam_scale: f32,
    /// Job-Startpunkt (mm) für den Laser-Tab: der Anker der Job-BBox bei
    /// relativem Startmodus. None = kein Marker (Absolut/leerer Job).
    pub job_start: Option<[f64; 2]>,
    /// Schwebender Haltesteg-Entwurf (nur beim Bridge-Werkzeug sichtbar).
    pub bridge: Option<super::state::BridgeDraft>,
}

/// Halbe Handle-Kantenlänge in Welt-mm, damit die sichtbare Fläche am
/// Bildschirm konstant etwa 10 px groß bleibt. Der Hit-Test ist großzügiger.
pub(crate) fn handle_hw(cam_scale: f32) -> f32 {
    5.0 / cam_scale
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

    // Job-Startmarker (Laser-Tab): grünes Fadenkreuz am Anker der Job-BBox —
    // dort setzt der Controller bei „Aktuelle Position"/„Benutzerursprung" an.
    if let Some([mx, my]) = input.job_start {
        let r = (10.0 / input.cam_scale) as f64;
        let green = [0.25, 0.7, 0.5, 1.0];
        scene_geo::push_seg(
            &mut v,
            [(mx - r) as f32, my as f32],
            [(mx + r) as f32, my as f32],
            green,
        );
        scene_geo::push_seg(
            &mut v,
            [mx as f32, (my - r) as f32],
            [mx as f32, (my + r) as f32],
            green,
        );
        // Kleines Quadrat um den Punkt, damit er auch über Konturen auffällt.
        let q = r * 0.4;
        let corners = [
            (mx - q, my - q),
            (mx + q, my - q),
            (mx + q, my + q),
            (mx - q, my + q),
            (mx - q, my - q),
        ];
        for w in corners.windows(2) {
            scene_geo::push_seg(
                &mut v,
                [w[0].0 as f32, w[0].1 as f32],
                [w[1].0 as f32, w[1].1 as f32],
                green,
            );
        }
    }

    let preview = [0.6, 0.8, 1.0, 0.9];
    // Das Auswahlrechteck ist reines, kameraabhängiges Feedback. Es gehört
    // deshalb ins Frame-Overlay und nicht in den gecachten Szenenpuffer.
    if let Drag::Marquee { start } = input.drag {
        let start = [start[0] as f32, start[1] as f32];
        let cur = [cur[0] as f32, cur[1] as f32];
        let corners = [start, [cur[0], start[1]], cur, [start[0], cur[1]], start];
        for edge in corners.windows(2) {
            dashed_seg(
                &mut v,
                edge[0],
                edge[1],
                scene_geo::SEL_BOX_COLOR,
                input.cam_scale,
            );
        }
    }

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

    // Haltesteg-Entwurf: rote Steg-Linie, grüne Bandkanten (dort wird die
    // Kontur quer geschlossen) und nachfassbare Endpunkt-Griffe.
    if input.tool == Tool::Bridge {
        if let Some(d) = input.bridge {
            const RED: [f32; 4] = [0.92, 0.15, 0.15, 1.0];
            const GREEN: [f32; 4] = [0.25, 0.85, 0.35, 0.95];
            let a = [d.p0[0] as f32, d.p0[1] as f32];
            let b = [d.p1[0] as f32, d.p1[1] as f32];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len > 1e-4 {
                scene_geo::push_seg(&mut v, a, b, RED);
                // Bandkanten: parallel im Abstand ±Breite/2.
                let n = [-dy / len, dx / len];
                let off = (d.width / 2.0) as f32;
                for s in [-1.0_f32, 1.0] {
                    let o = [n[0] * off * s, n[1] * off * s];
                    scene_geo::push_seg(
                        &mut v,
                        [a[0] + o[0], a[1] + o[1]],
                        [b[0] + o[0], b[1] + o[1]],
                        GREEN,
                    );
                }
            } else {
                // Klick (Null-Länge): Kreuz-Marker — der Commit legt die Linie
                // automatisch senkrecht über die nächste Konturkante.
                let r = 6.0 / input.cam_scale;
                scene_geo::push_seg(&mut v, [a[0] - r, a[1]], [a[0] + r, a[1]], RED);
                scene_geo::push_seg(&mut v, [a[0], a[1] - r], [a[0], a[1] + r], RED);
            }
            // Endpunkt-Griffe (bildschirmkonstant, wie Transform-Handles).
            let hw = handle_hw(input.cam_scale);
            for p in [a, b] {
                v.extend(scene_geo::rect_outline(
                    p[0] - hw,
                    p[1] - hw,
                    hw * 2.0,
                    hw * 2.0,
                    RED,
                ));
            }
        }
    }

    // Live-Vorschau des Punkt-Zugs (Polyline/Spline/Bézier/Polygon): gesetzte
    // Segmente + gestricheltes Gummiband zur Maus + Punkt-Marker, wie Tauri.
    if !input.poly_pts.is_empty() && matches!(input.tool, Tool::Polyline | Tool::Spline) {
        let col = [0.9, 0.9, 0.95, 0.9];
        let first = input.poly_pts[0];
        let snap_start = input.poly_pts.len() >= 3
            && (first.0 - cur[0]).hypot(first.1 - cur[1]) <= 10.0 / input.cam_scale as f64;
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
        let rubber_end = if snap_start {
            [first.0 as f32, first.1 as f32]
        } else {
            [cur[0] as f32, cur[1] as f32]
        };
        dashed_seg(
            &mut v,
            [last.0 as f32, last.1 as f32],
            rubber_end,
            [1.0, 1.0, 1.0, 0.4],
            input.cam_scale,
        );
        // Punkt-Marker (kleine Quadrate); Startpunkt hervorgehoben.
        let hw = 3.0 / input.cam_scale;
        for (i, p) in input.poly_pts.iter().enumerate() {
            let c = if i == 0 && snap_start {
                [0.25, 0.72, 0.5, 1.0] // Start grün (Schließen-Signal)
            } else {
                [0.3, 0.51, 0.97, 1.0]
            };
            v.extend(scene_geo::handle_marker(p.0 as f32, p.1 as f32, hw, c));
        }
    }

    if input.tool == Tool::Bezier && !input.bezier_nodes.is_empty() {
        let first = input.bezier_nodes[0].p;
        let snap_start = input.bezier_nodes.len() >= 3
            && (first.0 - cur[0]).hypot(first.1 - cur[1]) <= 10.0 / input.cam_scale as f64;
        let path = luxifer_core::bezier::BezierPath {
            nodes: input.bezier_nodes.to_vec(),
            closed: false,
        };
        let flat = path.flatten();
        for edge in flat.windows(2) {
            scene_geo::push_seg(
                &mut v,
                [edge[0].0 as f32, edge[0].1 as f32],
                [edge[1].0 as f32, edge[1].1 as f32],
                [0.9, 0.9, 0.95, 0.9],
            );
        }
        let last = input.bezier_nodes.last().unwrap();
        let rubber_end = if snap_start {
            [first.0 as f32, first.1 as f32]
        } else {
            [cur[0] as f32, cur[1] as f32]
        };
        dashed_seg(
            &mut v,
            [last.p.0 as f32, last.p.1 as f32],
            rubber_end,
            [1.0, 1.0, 1.0, 0.4],
            input.cam_scale,
        );
        let hw = 3.0 / input.cam_scale;
        for (i, node) in input.bezier_nodes.iter().enumerate() {
            for handle in [node.h_in, node.h_out].into_iter().flatten() {
                scene_geo::push_seg(
                    &mut v,
                    [node.p.0 as f32, node.p.1 as f32],
                    [handle.0 as f32, handle.1 as f32],
                    [0.55, 0.65, 0.8, 0.65],
                );
                v.extend(scene_geo::handle_marker(
                    handle.0 as f32,
                    handle.1 as f32,
                    hw * 0.75,
                    [0.55, 0.65, 0.8, 1.0],
                ));
            }
            let color = if i == 0 && snap_start {
                [0.25, 0.72, 0.5, 1.0]
            } else {
                [0.3, 0.51, 0.97, 1.0]
            };
            v.extend(scene_geo::handle_marker(
                node.p.0 as f32,
                node.p.1 as f32,
                hw,
                color,
            ));
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
    for (handle, (hx, hy)) in luxifer_core::Handle::positions(&b) {
        let (half_w, half_h) = if handle.is_corner() {
            (hw, hw)
        } else if matches!(handle, luxifer_core::Handle::N | luxifer_core::Handle::S) {
            (hw * 1.35, hw * 0.62)
        } else {
            (hw * 0.62, hw * 1.35)
        };
        v.extend(scene_geo::transform_handle(
            hx as f32,
            hy as f32,
            half_w,
            half_h,
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
    v.extend(scene_geo::rotation_handle(
        rp[0] as f32,
        rp[1] as f32,
        hw * 1.15,
        scene_geo::HANDLE_COLOR,
    ));
    v
}
