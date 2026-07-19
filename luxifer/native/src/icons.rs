//! Gemalte Icons für die Werkzeug-/Anordnen-Leisten, nachgebaut aus der
//! Tauri-`Icon.svelte` (Lucide-Stil, 24×24-stroke). Die Pfade sind hier mit
//! egui-Primitiven erkennbar nachgezeichnet — dieselbe Bedeutung, kein
//! SVG-Parser. `draw` rendert in ein Rect (Icon-Box), Farbe = currentColor.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

/// Zeichnet das Icon `name` in `rect` (typisch 24er-Box) mit `color`.
pub fn draw(p: &Painter, rect: Rect, name: &str, color: Color32) {
    // Lokales 24×24-Koordinatensystem → rect abbilden.
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.min
        + Vec2::new(
            (rect.width() - 24.0 * s) * 0.5,
            (rect.height() - 24.0 * s) * 0.5,
        );
    let pt = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let w = 1.8 * s;
    let st = Stroke::new(w, color);
    let line = |a: [f32; 2], b: [f32; 2]| {
        p.line_segment([pt(a[0], a[1]), pt(b[0], b[1])], st);
    };
    let poly = |pts: &[[f32; 2]], closed: bool| {
        let v: Vec<Pos2> = pts.iter().map(|q| pt(q[0], q[1])).collect();
        let shape = if closed {
            Shape::closed_line(v, st)
        } else {
            Shape::line(v, st)
        };
        p.add(shape);
    };
    let rrect = |x: f32, y: f32, ww: f32, hh: f32| {
        p.rect_stroke(
            Rect::from_min_size(pt(x, y), Vec2::new(ww * s, hh * s)),
            2.0,
            st,
            egui::StrokeKind::Inside,
        );
    };
    let circ = |cx: f32, cy: f32, r: f32| {
        p.circle_stroke(pt(cx, cy), r * s, st);
    };
    let dot = |cx: f32, cy: f32, r: f32| {
        p.circle_filled(pt(cx, cy), r * s, color);
    };

    match name {
        "assets" => {
            rrect(3.0, 4.0, 18.0, 16.0);
            circ(8.0, 9.0, 1.5);
            poly(
                &[
                    [5.0, 17.0],
                    [10.0, 12.0],
                    [13.0, 15.0],
                    [16.0, 11.0],
                    [20.0, 16.0],
                ],
                false,
            );
        }
        // ── Werkzeuge ──────────────────────────────────────────────────────
        "select" => {
            poly(
                &[[3.0, 3.0], [10.07, 19.97], [12.58, 12.58], [19.97, 10.07]],
                true,
            );
            line([13.0, 13.0], [19.0, 19.0]);
        }
        "rect" => rrect(3.0, 5.0, 18.0, 14.0),
        "ellipse" => {
            // Ellipse als 32-Segment-Polygon.
            let n = 32;
            let pts: Vec<Pos2> = (0..n)
                .map(|i| {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    pt(12.0 + 9.0 * a.cos(), 12.0 + 6.5 * a.sin())
                })
                .collect();
            p.add(Shape::closed_line(pts, st));
            // (add gibt ShapeIdx zurück; Semikolon macht den Arm zu ())
        }
        "polygon" => poly(
            &[
                [12.0, 2.0],
                [21.0, 8.5],
                [17.5, 19.0],
                [6.5, 19.0],
                [3.0, 8.5],
            ],
            true,
        ),
        "line" => line([4.0, 20.0], [20.0, 4.0]),
        "polyline" => poly(&[[3.0, 17.0], [9.0, 9.0], [13.0, 12.0], [21.0, 3.0]], false),
        "spline" => {
            poly(&[[4.0, 18.0], [8.0, 18.0], [10.0, 6.0], [14.0, 6.0]], false);
            dot(4.0, 18.0, 1.6);
            dot(14.0, 6.0, 1.6);
        }
        "bezier" => {
            poly(
                &[
                    [3.0, 16.0],
                    [8.0, 8.0],
                    [12.0, 8.0],
                    [16.0, 16.0],
                    [21.0, 16.0],
                ],
                false,
            );
            dot(3.0, 16.0, 1.4);
            dot(21.0, 16.0, 1.4);
        }
        "text" => {
            line([4.0, 5.5], [20.0, 5.5]);
            line([12.0, 5.0], [12.0, 19.0]);
            line([9.0, 19.0], [15.0, 19.0]);
        }
        "node" => {
            circ(12.0, 12.0, 2.4);
            line([12.0, 2.0], [12.0, 7.0]);
            line([12.0, 17.0], [12.0, 22.0]);
            line([2.0, 12.0], [7.0, 12.0]);
            line([17.0, 12.0], [22.0, 12.0]);
        }
        "trim" => {
            circ(6.0, 7.0, 2.4);
            circ(6.0, 17.0, 2.4);
            line([8.0, 8.5], [20.0, 17.0]);
            line([8.0, 15.5], [20.0, 7.0]);
        }
        "bridge" => {
            poly(
                &[[3.0, 16.0], [3.0, 14.0], [21.0, 14.0], [21.0, 16.0]],
                false,
            );
            line([3.0, 16.0], [21.0, 16.0]);
            line([9.0, 16.0], [9.0, 20.0]);
            line([15.0, 16.0], [15.0, 20.0]);
        }
        "boolean" => {
            circ(9.0, 12.0, 6.0);
            circ(15.0, 12.0, 6.0);
        }
        "fillet" => poly(&[[5.0, 19.0], [5.0, 11.0], [11.0, 5.0], [19.0, 5.0]], false),
        "pattern-fill" => {
            rrect(3.0, 3.0, 18.0, 18.0);
            line([3.0, 9.0], [21.0, 9.0]);
            line([3.0, 15.0], [21.0, 15.0]);
            line([9.0, 3.0], [9.0, 21.0]);
            line([15.0, 3.0], [15.0, 21.0]);
        }
        "offset" => {
            rrect(7.0, 7.0, 12.0, 12.0);
            poly(&[[4.0, 16.0], [4.0, 4.0], [16.0, 4.0]], false);
        }
        "measure" => {
            rrect(3.0, 8.0, 18.0, 8.0);
            for x in [7.0, 11.0, 15.0, 19.0] {
                line([x, 8.0], [x, 12.0]);
            }
        }
        "mirror-h" => {
            line([12.0, 3.0], [12.0, 21.0]);
            poly(&[[8.0, 7.0], [3.0, 12.0], [8.0, 17.0]], true);
            poly(&[[16.0, 7.0], [21.0, 12.0], [16.0, 17.0]], true);
        }
        "mirror-v" => {
            line([3.0, 12.0], [21.0, 12.0]);
            poly(&[[7.0, 8.0], [12.0, 3.0], [17.0, 8.0]], true);
            poly(&[[7.0, 16.0], [12.0, 21.0], [17.0, 16.0]], true);
        }
        "coaster-rect" => {
            rrect(3.0, 4.0, 8.0, 7.0);
            rrect(13.0, 4.0, 8.0, 7.0);
            rrect(3.0, 13.0, 8.0, 7.0);
            rrect(13.0, 13.0, 8.0, 7.0);
        }
        "coaster-circle" => {
            circ(7.0, 7.5, 3.5);
            circ(17.0, 7.5, 3.5);
            circ(7.0, 16.5, 3.5);
            circ(17.0, 16.5, 3.5);
        }

        // ── Anordnen ───────────────────────────────────────────────────────
        "align-left" => {
            line([4.0, 4.0], [4.0, 20.0]);
            rrect(7.0, 6.0, 12.0, 4.0);
            rrect(7.0, 14.0, 7.0, 4.0);
        }
        "align-right" => {
            line([20.0, 4.0], [20.0, 20.0]);
            rrect(5.0, 6.0, 12.0, 4.0);
            rrect(10.0, 14.0, 7.0, 4.0);
        }
        "align-hcenter" => {
            line([12.0, 3.0], [12.0, 21.0]);
            rrect(6.0, 6.0, 12.0, 4.0);
            rrect(8.5, 14.0, 7.0, 4.0);
        }
        "align-top" => {
            line([4.0, 4.0], [20.0, 4.0]);
            rrect(6.0, 7.0, 4.0, 12.0);
            rrect(14.0, 7.0, 4.0, 7.0);
        }
        "align-bottom" => {
            line([4.0, 20.0], [20.0, 20.0]);
            rrect(6.0, 5.0, 4.0, 12.0);
            rrect(14.0, 10.0, 4.0, 7.0);
        }
        "align-vcenter" => {
            line([3.0, 12.0], [21.0, 12.0]);
            rrect(6.0, 6.0, 4.0, 12.0);
            rrect(14.0, 8.5, 4.0, 7.0);
        }
        "align-center" => {
            line([12.0, 3.0], [12.0, 21.0]);
            line([3.0, 12.0], [21.0, 12.0]);
            rrect(8.0, 8.0, 8.0, 8.0);
        }
        "dist-h" => {
            rrect(3.0, 7.0, 4.0, 10.0);
            rrect(10.0, 7.0, 4.0, 10.0);
            rrect(17.0, 7.0, 4.0, 10.0);
        }
        "dist-v" => {
            rrect(7.0, 3.0, 10.0, 4.0);
            rrect(7.0, 10.0, 10.0, 4.0);
            rrect(7.0, 17.0, 10.0, 4.0);
        }
        "space-h" => {
            line([3.0, 4.0], [3.0, 20.0]);
            line([21.0, 4.0], [21.0, 20.0]);
            rrect(7.0, 8.0, 10.0, 8.0);
        }
        "space-v" => {
            line([4.0, 3.0], [20.0, 3.0]);
            line([4.0, 21.0], [20.0, 21.0]);
            rrect(8.0, 7.0, 8.0, 10.0);
        }
        "group" => {
            rrect(4.0, 4.0, 9.0, 9.0);
            rrect(11.0, 11.0, 9.0, 9.0);
        }
        "ungroup" => {
            rrect(3.0, 3.0, 8.0, 8.0);
            rrect(13.0, 13.0, 8.0, 8.0);
        }
        "nest" => {
            rrect(3.0, 3.0, 18.0, 18.0);
            rrect(6.0, 6.0, 6.0, 6.0);
            rrect(13.0, 7.0, 5.0, 5.0);
            rrect(8.0, 14.0, 8.0, 4.0);
        }
        "nest-fill" => {
            rrect(3.0, 3.0, 18.0, 18.0);
            rrect(5.0, 5.0, 6.0, 6.0);
            rrect(13.0, 5.0, 6.0, 6.0);
            rrect(5.0, 13.0, 6.0, 6.0);
            rrect(13.0, 13.0, 6.0, 6.0);
        }
        "lock" => {
            rrect(5.0, 10.0, 14.0, 11.0);
            poly(
                &[
                    [8.0, 10.0],
                    [8.0, 7.0],
                    [10.0, 4.0],
                    [14.0, 4.0],
                    [16.0, 7.0],
                    [16.0, 10.0],
                ],
                false,
            );
            line([12.0, 14.0], [12.0, 17.0]);
        }
        "unlock" => {
            rrect(5.0, 10.0, 14.0, 11.0);
            poly(
                &[
                    [16.0, 10.0],
                    [16.0, 7.0],
                    [14.0, 4.0],
                    [10.0, 4.0],
                    [8.0, 7.0],
                ],
                false,
            );
            line([12.0, 14.0], [12.0, 17.0]);
        }
        // ── Polygon-Formen (Flyout) ────────────────────────────────────────
        "tri" => poly(&[[12.0, 4.0], [21.0, 19.0], [3.0, 19.0]], true),
        "quad" => poly(
            &[[12.0, 3.0], [21.0, 12.0], [12.0, 21.0], [3.0, 12.0]],
            true,
        ),
        "penta" => poly(
            &[
                [12.0, 3.0],
                [21.0, 10.0],
                [17.5, 21.0],
                [6.5, 21.0],
                [3.0, 10.0],
            ],
            true,
        ),
        "hex" => poly(
            &[
                [8.0, 4.0],
                [16.0, 4.0],
                [20.0, 12.0],
                [16.0, 20.0],
                [8.0, 20.0],
                [4.0, 12.0],
            ],
            true,
        ),
        "octa" => poly(
            &[
                [8.0, 3.0],
                [16.0, 3.0],
                [21.0, 8.0],
                [21.0, 16.0],
                [16.0, 21.0],
                [8.0, 21.0],
                [3.0, 16.0],
                [3.0, 8.0],
            ],
            true,
        ),
        "star" => {
            // 5-zackiger Stern.
            let mut pts = Vec::new();
            for i in 0..10 {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let rr = if i % 2 == 0 { 9.0 } else { 3.6 };
                pts.push([12.0 + rr * a.cos(), 12.0 + rr * a.sin()]);
            }
            poly(&pts, true);
        }
        "sun" => {
            circ(12.0, 12.0, 4.0);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                line(
                    [12.0 + 6.0 * a.cos(), 12.0 + 6.0 * a.sin()],
                    [12.0 + 9.0 * a.cos(), 12.0 + 9.0 * a.sin()],
                );
            }
        }
        "gear" => {
            circ(12.0, 12.0, 3.0);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                line(
                    [12.0 + 5.0 * a.cos(), 12.0 + 5.0 * a.sin()],
                    [12.0 + 8.5 * a.cos(), 12.0 + 8.5 * a.sin()],
                );
            }
        }
        "heart" => poly(
            &[
                [12.0, 20.0],
                [4.0, 11.0],
                [4.0, 6.5],
                [8.0, 5.0],
                [12.0, 8.5],
                [16.0, 5.0],
                [20.0, 6.5],
                [20.0, 11.0],
            ],
            true,
        ),

        // ── Kopfzeile ──────────────────────────────────────────────────────
        "new-file" => {
            // Leeres Dokument mit umgeknickter Ecke und kleinem Plus.
            poly(
                &[
                    [5.0, 3.0],
                    [14.0, 3.0],
                    [19.0, 8.0],
                    [19.0, 21.0],
                    [5.0, 21.0],
                    [5.0, 3.0],
                ],
                false,
            );
            poly(&[[14.0, 3.0], [14.0, 8.0], [19.0, 8.0]], false);
            line([8.0, 14.0], [16.0, 14.0]);
            line([12.0, 10.0], [12.0, 18.0]);
        }
        "undo" => {
            // Pfeil nach links + Bogen (Lucide undo-2), Bogen als Polylinie.
            poly(&[[9.0, 14.0], [4.0, 9.0], [9.0, 4.0]], false);
            let mut pts = vec![pt(4.0, 9.0), pt(14.5, 9.0)];
            let n = 12;
            for i in 1..=n {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::PI;
                pts.push(pt(14.5 + 5.5 * a.cos(), 14.5 + 5.5 * a.sin()));
            }
            pts.push(pt(11.0, 20.0));
            p.add(Shape::line(pts, st));
        }
        "redo" => {
            // Gespiegeltes undo: Pfeil nach rechts, Bogen links herum.
            poly(&[[15.0, 14.0], [20.0, 9.0], [15.0, 4.0]], false);
            let mut pts = vec![pt(20.0, 9.0), pt(9.5, 9.0)];
            let n = 12;
            for i in 1..=n {
                let a = -std::f32::consts::FRAC_PI_2 - i as f32 / n as f32 * std::f32::consts::PI;
                pts.push(pt(9.5 + 5.5 * a.cos(), 14.5 + 5.5 * a.sin()));
            }
            pts.push(pt(13.0, 20.0));
            p.add(Shape::line(pts, st));
        }
        "import" => {
            // Pfeil nach unten in eine Ablage (Lucide download).
            line([12.0, 3.0], [12.0, 14.0]);
            poly(&[[7.0, 9.0], [12.0, 14.0], [17.0, 9.0]], false);
            poly(
                &[[3.0, 15.0], [3.0, 20.0], [21.0, 20.0], [21.0, 15.0]],
                false,
            );
        }

        // Fallback: kleiner Punkt.
        _ => dot(12.0, 12.0, 2.0),
    }
}
