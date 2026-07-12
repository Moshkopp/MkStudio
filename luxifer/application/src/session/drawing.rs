use super::{BoxShape, EditorSession, PointPath};

impl EditorSession {
    pub fn add_box_shape(
        &mut self,
        shape: BoxShape,
        start: [f64; 2],
        end: [f64; 2],
    ) -> Option<usize> {
        let x = start[0].min(end[0]);
        let y = start[1].min(end[1]);
        let w = (start[0] - end[0]).abs();
        let h = (start[1] - end[1]).abs();
        if w < 0.5 || h < 0.5 {
            return None;
        }
        let geometry = match shape {
            BoxShape::Rect => luxifer_core::Geo::Rect { x, y, w, h },
            BoxShape::Ellipse => luxifer_core::Geo::Ellipse {
                cx: x + w / 2.0,
                cy: y + h / 2.0,
                rx: w / 2.0,
                ry: h / 2.0,
            },
        };
        Some(self.state.add_shape(geometry))
    }

    pub fn add_line(&mut self, start: [f64; 2], end: [f64; 2]) -> Option<usize> {
        if (start[0] - end[0]).hypot(start[1] - end[1]) < 0.5 {
            return None;
        }
        Some(self.state.add_shape(luxifer_core::Geo::Polyline {
            pts: vec![(start[0], start[1]), (end[0], end[1])],
            closed: false,
        }))
    }

    pub fn add_polygon(
        &mut self,
        shape: luxifer_core::PolyShape,
        center: [f64; 2],
        edge: [f64; 2],
    ) -> Option<usize> {
        let radius = (center[0] - edge[0]).hypot(center[1] - edge[1]);
        if radius < 1.0 {
            return None;
        }
        let pts = shape.points(center[0], center[1], radius, 0.0);
        Some(
            self.state
                .add_shape(luxifer_core::Geo::Polyline { pts, closed: true }),
        )
    }

    pub fn add_point_path(&mut self, path: PointPath, points: Vec<(f64, f64)>) -> Option<usize> {
        if points.len() < 2 {
            return None;
        }
        let index = match path {
            PointPath::Polyline => self.state.add_shape(luxifer_core::Geo::Polyline {
                pts: points,
                closed: false,
            }),
            PointPath::Spline => {
                let pts = luxifer_core::geometry::catmull_rom(&points, false, 12);
                self.state
                    .add_shape(luxifer_core::Geo::Polyline { pts, closed: false })
            }
            PointPath::Bezier => self.state.add_bezier(points, false),
        };
        Some(index)
    }
}
