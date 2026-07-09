//! Laser-Preview (ADR 0005): eine **abgeleitete Sicht** auf den `JobPlan`.
//!
//! Der Core wandelt Shapes + Layer in einen `JobPlan` (job.rs). Diese Datei
//! leitet daraus die zu fahrenden Linien in **Ausführungsreihenfolge** ab —
//! inklusive der **Verfahrwege (Travel)** zwischen den Arbeitssegmenten. Das
//! Frontend zeichnet nur diese Segmente; es gibt keine zweite Pfad-Berechnung
//! (CLAUDE.md Regel 1 & 2).
//!
//! Die Reihenfolge ist die **Plan-Reihenfolge** = die vom Nutzer gesetzte
//! Layer-Reihenfolge (ADR 0005 §0). Die Preview ordnet nicht selbst um.

use crate::geometry::Pt;
use crate::job::{JobPlan, LayerWork};

/// Art eines Bewegungssegments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    /// Kontur fahren (Laser an).
    Cut,
    /// Scanline fahren (Laser an).
    Fill,
    /// Rasterzeile fahren (Laser moduliert) — folgt mit dem Bild-Job
    /// (ADR 0004 §5). Wird erst befüllt, wenn `LayerWork::Raster` existiert.
    Raster,
    /// Leerfahrt (Laser aus) zwischen zwei Arbeitssegmenten.
    Travel,
}

/// Ein Bewegungssegment der Vorschau in mm, in Ausführungsreihenfolge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewMove {
    pub from: Pt,
    pub to: Pt,
    pub kind: MoveKind,
    /// Welcher Layer (für Einfärbung/Filter). Bei `Travel` der Ziel-Layer.
    pub layer_id: usize,
    /// Globaler Reihenfolge-Index (0..n) für Reihenfolge/Scrubber.
    pub seq: u32,
}

impl PreviewMove {
    /// Länge des Segments in mm.
    pub fn len_mm(&self) -> f64 {
        let dx = self.to.0 - self.from.0;
        let dy = self.to.1 - self.from.1;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Die komplette Vorschau eines Jobs (abgeleitet aus `JobPlan`).
#[derive(Debug, Clone, PartialEq)]
pub struct JobPreview {
    pub moves: Vec<PreviewMove>,
    /// Bounding-Box aller Geometrie (mm), aus dem Plan übernommen.
    pub bbox: Option<(f64, f64, f64, f64)>,
    /// Summe aller Segmentlängen (Arbeit + Travel) in mm.
    pub total_len_mm: f64,
}

impl JobPreview {
    /// Leitet die Vorschau aus einem `JobPlan` ab. Läuft die Layer in
    /// Plan-Reihenfolge durch, wandelt Cut-Pfade und Fill-Segmente in
    /// `PreviewMove`s und fügt vor jedem Arbeitssegment einen `Travel` ein, wenn
    /// dessen Startpunkt nicht schon der aktuellen Kopf-Position entspricht.
    pub fn from_plan(plan: &JobPlan) -> JobPreview {
        let mut moves: Vec<PreviewMove> = Vec::new();
        let mut seq: u32 = 0;
        // Aktuelle Kopf-Position; None = noch nichts gefahren (kein Anfahr-Travel
        // ab dem Ursprung, das erste Arbeitssegment beginnt einfach dort).
        let mut head: Option<Pt> = None;

        let emit = |moves: &mut Vec<PreviewMove>,
                    head: &mut Option<Pt>,
                    seq: &mut u32,
                    from: Pt,
                    to: Pt,
                    kind: MoveKind,
                    layer_id: usize| {
            // Travel einfügen, wenn der Kopf woanders steht als der Startpunkt.
            if let Some(h) = *head {
                if h != from {
                    moves.push(PreviewMove {
                        from: h,
                        to: from,
                        kind: MoveKind::Travel,
                        layer_id,
                        seq: *seq,
                    });
                    *seq += 1;
                }
            }
            moves.push(PreviewMove {
                from,
                to,
                kind,
                layer_id,
                seq: *seq,
            });
            *seq += 1;
            *head = Some(to);
        };

        for jl in &plan.layers {
            match &jl.work {
                LayerWork::Cut { paths } => {
                    for path in paths {
                        let pts = &path.points;
                        if pts.len() < 2 {
                            continue;
                        }
                        for w in pts.windows(2) {
                            emit(
                                &mut moves,
                                &mut head,
                                &mut seq,
                                w[0],
                                w[1],
                                MoveKind::Cut,
                                jl.layer_id,
                            );
                        }
                        // Geschlossene Kontur: letztes Segment zurück zum Start.
                        if path.closed {
                            emit(
                                &mut moves,
                                &mut head,
                                &mut seq,
                                pts[pts.len() - 1],
                                pts[0],
                                MoveKind::Cut,
                                jl.layer_id,
                            );
                        }
                    }
                }
                LayerWork::Fill { segments } => {
                    for s in segments {
                        emit(
                            &mut moves,
                            &mut head,
                            &mut seq,
                            (s.x0, s.y),
                            (s.x1, s.y),
                            MoveKind::Fill,
                            jl.layer_id,
                        );
                    }
                } // Raster folgt mit dem Bild-Job (ADR 0004 §5, ADR 0005 §3).
            }
        }

        let total_len_mm = moves.iter().map(|m| m.len_mm()).sum();
        JobPreview {
            moves,
            bbox: plan.bbox,
            total_len_mm,
        }
    }

    /// Ob die Vorschau überhaupt Bewegungen enthält.
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Geo;
    use crate::state::AppState;

    fn plan_one_rect() -> JobPlan {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 20.0,
        });
        JobPlan::from_shapes(&s.shapes, &s.layers)
    }

    #[test]
    fn rechteck_cut_ergibt_vier_geschlossene_segmente() {
        let preview = JobPreview::from_plan(&plan_one_rect());
        let cuts: Vec<_> = preview
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Cut)
            .collect();
        // 4 Ecken → 3 Kanten-Segmente + 1 Schließ-Segment = 4.
        assert_eq!(cuts.len(), 4);
        // Ein einzelner zusammenhängender Pfad → keine Travel-Sprünge.
        assert!(
            !preview.moves.iter().any(|m| m.kind == MoveKind::Travel),
            "zusammenhängende Kontur braucht keine Leerfahrt"
        );
        // Letztes Cut-Segment schließt zurück zum Startpunkt.
        let last = cuts.last().unwrap();
        assert_eq!(last.to, (0.0, 0.0));
    }

    #[test]
    fn seq_ist_lueckenlos_und_aufsteigend() {
        let preview = JobPreview::from_plan(&plan_one_rect());
        for (i, m) in preview.moves.iter().enumerate() {
            assert_eq!(m.seq as usize, i, "seq muss 0..n lückenlos sein");
        }
    }

    #[test]
    fn zwei_getrennte_pfade_erzeugen_travel_dazwischen() {
        // Zwei Rechtecke, weit auseinander, auf demselben Layer.
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        let c = s.layers[0].color;
        s.selected.clear();
        s.activate_color(c); // gleiche Farbe → gleicher Layer
        s.add_shape(Geo::Rect {
            x: 100.0,
            y: 100.0,
            w: 10.0,
            h: 10.0,
        });
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        let preview = JobPreview::from_plan(&plan);

        let travels: Vec<_> = preview
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Travel)
            .collect();
        // Genau eine Leerfahrt: vom Ende des ersten zum Start des zweiten Rechtecks.
        assert_eq!(travels.len(), 1, "eine Leerfahrt zwischen den Konturen");
        assert_eq!(travels[0].from, (0.0, 0.0), "Ende der ersten Kontur");
        assert_eq!(travels[0].to, (100.0, 100.0), "Start der zweiten Kontur");
    }

    #[test]
    fn reihenfolge_folgt_layer_reihenfolge() {
        // Zwei Layer; nach move_layer muss die Preview den umsortierten Layer
        // zuerst zeigen.
        let mut s = AppState::new();
        s.selected.clear();
        s.activate_color([10, 0, 0]);
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 5.0,
            h: 5.0,
        });
        s.selected.clear();
        s.activate_color([0, 20, 0]);
        s.add_shape(Geo::Rect {
            x: 50.0,
            y: 50.0,
            w: 5.0,
            h: 5.0,
        });

        // Vorher: Layer 0 (rot) zuerst → erstes Segment gehört layer_id 0.
        let before = JobPreview::from_plan(&JobPlan::from_shapes(&s.shapes, &s.layers));
        assert_eq!(before.moves[0].layer_id, 0);

        // Zweiten Layer nach vorne holen.
        s.move_layer(1, 0);
        let after = JobPreview::from_plan(&JobPlan::from_shapes(&s.shapes, &s.layers));
        // Jetzt gehört das erste Segment dem Layer, der auf Index 0 liegt.
        assert_eq!(after.moves[0].layer_id, 0);
        // Und dieser Layer ist der grüne (vormals Index 1).
        assert_eq!(s.layers[0].color, [0, 20, 0]);
    }

    #[test]
    fn fill_layer_erzeugt_fill_segmente() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        s.layers[0].mode = crate::model::LayerMode::Fill;
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        let preview = JobPreview::from_plan(&plan);
        assert!(
            preview.moves.iter().any(|m| m.kind == MoveKind::Fill),
            "Fill-Layer erzeugt Fill-Segmente"
        );
        assert!(!preview.moves.iter().any(|m| m.kind == MoveKind::Cut));
    }

    #[test]
    fn leerer_plan_ergibt_leere_preview() {
        let plan = JobPlan::from_shapes(&[], &[]);
        let preview = JobPreview::from_plan(&plan);
        assert!(preview.is_empty());
        assert_eq!(preview.total_len_mm, 0.0);
        assert_eq!(preview.bbox, None);
    }

    #[test]
    fn total_len_summiert_arbeit_und_travel() {
        let preview = JobPreview::from_plan(&plan_one_rect());
        // Rechteck 10x20 → Umfang 60 mm, keine Travel.
        assert!((preview.total_len_mm - 60.0).abs() < 1e-6);
    }
}
