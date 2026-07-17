//! Klickbasiertes Trim: Zielabschnitt zwischen den nächsten Schnittpunkten
//! bestimmen und als normale offene Polylinien ersetzen (ADR 0016).

use crate::geometry::{point_segment_distance, rotate_point, Geo, Pt};
use crate::model::Shape;
use crate::state::AppState;

const EPS: f64 = 1e-7;

#[derive(Debug, Clone, PartialEq)]
pub struct TrimPreview {
    pub target: usize,
    pub removed: Vec<Pt>,
    pub remaining: Vec<Vec<Pt>>,
}

impl AppState {
    pub fn trim_preview(&self, click: Pt, tolerance: f64) -> Option<TrimPreview> {
        let mut candidates = Vec::new();
        for (index, shape) in self.shapes.iter().enumerate() {
            let Some(layer) = self.layers.get(shape.layer_id) else {
                continue;
            };
            if !layer.visible
                || layer.locked
                || shape.fill_only
                || matches!(shape.geo, Geo::Image { .. })
            {
                continue;
            }
            let (points, closed) = world_path(shape);
            if let Some((distance, click_s)) = nearest_path_position(&points, closed, click) {
                if distance <= tolerance {
                    candidates.push((distance, index, points, closed, click_s));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
        for (_, target, points, closed, click_s) in candidates {
            let cutters: Vec<(Vec<Pt>, bool)> = self
                .shapes
                .iter()
                .enumerate()
                .filter(|(index, shape)| {
                    *index != target
                        && !shape.fill_only
                        && !matches!(shape.geo, Geo::Image { .. })
                        && self
                            .layers
                            .get(shape.layer_id)
                            .is_some_and(|layer| layer.visible && !layer.locked)
                })
                .map(|(_, shape)| world_path(shape))
                .collect();
            if let Some((removed, remaining)) = trim_path(&points, closed, click_s, &cutters) {
                return Some(TrimPreview {
                    target,
                    removed,
                    remaining,
                });
            }
        }
        None
    }

    pub fn trim_at(&mut self, click: Pt, tolerance: f64) -> bool {
        self.apply_trim(click, tolerance, true)
    }

    /// Variante fuer eine bereits vom Aufrufer geoeffnete Undo-Geste.
    pub fn trim_at_in_edit(&mut self, click: Pt, tolerance: f64) -> bool {
        self.apply_trim(click, tolerance, false)
    }

    fn apply_trim(&mut self, click: Pt, tolerance: f64, record_undo: bool) -> bool {
        let Some(preview) = self.trim_preview(click, tolerance) else {
            return false;
        };
        let original = self.shapes[preview.target].clone();
        if record_undo {
            self.push_undo();
        }
        self.shapes.remove(preview.target);
        self.selected.clear();
        for points in preview.remaining {
            if points.len() < 2 {
                continue;
            }
            let mut shape = original.clone();
            shape.geo = Geo::Polyline {
                pts: points,
                closed: false,
            };
            shape.rotation = 0.0;
            shape.bezier = None;
            shape.text_meta = None;
            self.shapes.push(shape);
        }
        self.invalidate_shape_bounds();
        self.dirty = true;
        true
    }
}

fn world_path(shape: &Shape) -> (Vec<Pt>, bool) {
    let (mut points, closed) = shape.geo.outline_points();
    if shape.rotation != 0.0 {
        let center = shape.geo.bbox().center();
        for point in &mut points {
            *point = rotate_point(point.0, point.1, center.0, center.1, shape.rotation);
        }
    }
    (points, closed)
}

fn segments(points: &[Pt], closed: bool) -> impl Iterator<Item = (usize, Pt, Pt)> + '_ {
    let count = points.len().saturating_sub(1) + usize::from(closed && points.len() > 2);
    (0..count).map(move |index| (index, points[index], points[(index + 1) % points.len()]))
}

fn cumulative(points: &[Pt], closed: bool) -> (Vec<f64>, f64) {
    let mut starts = Vec::new();
    let mut total = 0.0;
    for (_, a, b) in segments(points, closed) {
        starts.push(total);
        total += (b.0 - a.0).hypot(b.1 - a.1);
    }
    (starts, total)
}

fn nearest_path_position(points: &[Pt], closed: bool, click: Pt) -> Option<(f64, f64)> {
    let (starts, _) = cumulative(points, closed);
    segments(points, closed)
        .filter_map(|(index, a, b)| {
            let dx = b.0 - a.0;
            let dy = b.1 - a.1;
            let len2 = dx * dx + dy * dy;
            if len2 <= EPS {
                return None;
            }
            let t = (((click.0 - a.0) * dx + (click.1 - a.1) * dy) / len2).clamp(0.0, 1.0);
            Some((
                point_segment_distance(click.0, click.1, a, b),
                starts[index] + t * len2.sqrt(),
            ))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

fn segment_intersection(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<(f64, f64)> {
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (d.0 - c.0, d.1 - c.1);
    let cross = |u: Pt, v: Pt| u.0 * v.1 - u.1 * v.0;
    let denominator = cross(r, s);
    if denominator.abs() <= EPS {
        return None;
    }
    let ca = (c.0 - a.0, c.1 - a.1);
    let t = cross(ca, s) / denominator;
    let u = cross(ca, r) / denominator;
    ((-EPS..=1.0 + EPS).contains(&t) && (-EPS..=1.0 + EPS).contains(&u))
        .then_some((t.clamp(0.0, 1.0), u.clamp(0.0, 1.0)))
}

fn trim_path(
    points: &[Pt],
    closed: bool,
    click_s: f64,
    cutters: &[(Vec<Pt>, bool)],
) -> Option<(Vec<Pt>, Vec<Vec<Pt>>)> {
    let (starts, total) = cumulative(points, closed);
    if total <= EPS {
        return None;
    }
    let mut cuts = Vec::new();
    for (index, a, b) in segments(points, closed) {
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        for (cutter, cutter_closed) in cutters {
            for (_, c, d) in segments(cutter, *cutter_closed) {
                if let Some((t, _)) = segment_intersection(a, b, c, d) {
                    cuts.push(starts[index] + t * length);
                }
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() <= EPS);
    if closed {
        if cuts.len() < 2 {
            return None;
        }
        let before = cuts
            .iter()
            .copied()
            .rfind(|s| *s <= click_s + EPS)
            .unwrap_or(cuts[cuts.len() - 1] - total);
        let after = cuts
            .iter()
            .copied()
            .find(|s| *s > click_s + EPS)
            .unwrap_or(cuts[0] + total);
        let removed = extract(points, true, before, after, total);
        let remaining = extract(points, true, after, before + total, total);
        (removed.len() >= 2 && remaining.len() >= 2).then_some((removed, vec![remaining]))
    } else {
        // Bei offenen Ketten sind auch die beiden freien Enden eindeutige
        // Begrenzungen. So lassen sich einseitige Ueberstaende und komplett
        // freie Ketten trimmen.
        let before = cuts
            .iter()
            .copied()
            .rfind(|s| *s <= click_s + EPS)
            .unwrap_or(0.0);
        let after = cuts
            .iter()
            .copied()
            .find(|s| *s > click_s + EPS)
            .unwrap_or(total);
        let removed = extract(points, false, before, after, total);
        let mut remaining = Vec::new();
        let left = extract(points, false, 0.0, before, total);
        let right = extract(points, false, after, total, total);
        if path_length(&left) > EPS {
            remaining.push(left);
        }
        if path_length(&right) > EPS {
            remaining.push(right);
        }
        (removed.len() >= 2 && path_length(&removed) > EPS).then_some((removed, remaining))
    }
}

fn point_at(points: &[Pt], closed: bool, s: f64, total: f64) -> Pt {
    let target = if closed {
        s.rem_euclid(total)
    } else {
        s.clamp(0.0, total)
    };
    let (starts, _) = cumulative(points, closed);
    for (index, a, b) in segments(points, closed) {
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if target <= starts[index] + len + EPS {
            let t = if len <= EPS {
                0.0
            } else {
                ((target - starts[index]) / len).clamp(0.0, 1.0)
            };
            return (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        }
    }
    *points.last().unwrap()
}

fn extract(points: &[Pt], closed: bool, start: f64, end: f64, total: f64) -> Vec<Pt> {
    let mut out = vec![point_at(points, closed, start, total)];
    let (starts, _) = cumulative(points, closed);
    let cycles = if closed { 2 } else { 1 };
    for cycle in 0..cycles {
        for (index, _, b) in segments(points, closed) {
            let at = starts[index]
                + (b.0 - points[index].0).hypot(b.1 - points[index].1)
                + cycle as f64 * total;
            if at > start + EPS && at < end - EPS {
                out.push(b);
            }
        }
    }
    out.push(point_at(points, closed, end, total));
    out.dedup_by(|a, b| (a.0 - b.0).hypot(a.1 - b.1) <= EPS);
    out
}

fn path_length(points: &[Pt]) -> f64 {
    points
        .windows(2)
        .map(|edge| (edge[1].0 - edge[0].0).hypot(edge[1].1 - edge[0].1))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offene_linie_wird_zwischen_zwei_schneidern_getrimmt() {
        let target = vec![(0.0, 0.0), (100.0, 0.0)];
        let cutters = vec![
            (vec![(30.0, -10.0), (30.0, 10.0)], false),
            (vec![(70.0, -10.0), (70.0, 10.0)], false),
        ];
        let (removed, remaining) = trim_path(&target, false, 50.0, &cutters).unwrap();
        assert_eq!(removed, vec![(30.0, 0.0), (70.0, 0.0)]);
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn fehlende_beidseitige_begrenzung_trimt_nicht() {
        let target = vec![(0.0, 0.0), (100.0, 0.0)];
        let cutters = vec![(vec![(30.0, -10.0), (30.0, 10.0)], false)];
        let (removed, remaining) = trim_path(&target, false, 10.0, &cutters).unwrap();
        assert_eq!(removed, vec![(0.0, 0.0), (30.0, 0.0)]);
        assert_eq!(remaining, vec![vec![(30.0, 0.0), (100.0, 0.0)]]);
    }

    #[test]
    fn freie_offene_kette_wird_vollstaendig_entfernt() {
        let target = vec![(0.0, 0.0), (40.0, 0.0), (100.0, 20.0)];
        let (removed, remaining) = trim_path(&target, false, 50.0, &[]).unwrap();
        assert_eq!(removed, target);
        assert!(remaining.is_empty());
    }

    #[test]
    fn geschlossene_kontur_braucht_zwei_schnittpunkte() {
        let target = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let cutters = vec![(vec![(30.0, -10.0), (30.0, 10.0)], false)];
        assert!(trim_path(&target, true, 50.0, &cutters).is_none());
    }

    #[test]
    fn geschlossene_kontur_liefert_offenen_restpfad() {
        let target = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let cutters = vec![
            (vec![(30.0, -10.0), (30.0, 110.0)], false),
            (vec![(70.0, -10.0), (70.0, 110.0)], false),
        ];
        let (removed, remaining) = trim_path(&target, true, 50.0, &cutters).unwrap();
        assert_eq!(removed, vec![(30.0, 0.0), (70.0, 0.0)]);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].first(), Some(&(70.0, 0.0)));
        assert_eq!(remaining[0].last(), Some(&(30.0, 0.0)));
    }

    #[test]
    fn trim_at_ist_ein_undo_schritt() {
        let mut state = AppState::new();
        state.add_shape(Geo::Polyline {
            pts: vec![(0.0, 0.0), (100.0, 0.0)],
            closed: false,
        });
        state.add_shape(Geo::Polyline {
            pts: vec![(30.0, -10.0), (30.0, 10.0)],
            closed: false,
        });
        state.add_shape(Geo::Polyline {
            pts: vec![(70.0, -10.0), (70.0, 10.0)],
            closed: false,
        });
        let before = state.shapes.clone();
        assert!(state.trim_at((50.0, 0.0), 1.0));
        assert_eq!(
            state.shapes.len(),
            4,
            "Ziel wird durch zwei Restpfade ersetzt"
        );
        assert!(state.selected.is_empty());
        state.undo();
        assert_eq!(state.shapes, before);
    }
}
