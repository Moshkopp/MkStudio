use crate::AppError;

use super::EditorSession;

impl EditorSession {
    pub fn trim(&mut self, point: (f64, f64), tolerance: f64) -> Result<(), AppError> {
        if self.state.trim_at(point, tolerance) {
            Ok(())
        } else {
            Err(AppError::new(
                "trim_no_segment",
                "Kein trimmbarer Abschnitt gefunden.",
            ))
        }
    }

    /// Trimmt innerhalb einer mit `begin_edit` gestarteten Pinsel-Geste, ohne
    /// fuer jeden Abschnitt einen eigenen Undo-Schritt anzulegen.
    pub fn trim_edit(&mut self, point: (f64, f64), tolerance: f64) -> bool {
        debug_assert!(self.edit_active(), "trim_edit ohne begin_edit");
        self.state.trim_at_in_edit(point, tolerance)
    }

    pub fn activate_color(&mut self, color: [u8; 3]) {
        self.state.activate_color(color);
    }

    pub fn mirror(&mut self, axis: luxifer_core::Axis) -> Result<(), AppError> {
        self.require_selection("Spiegeln")?;
        self.state.mirror_selection(axis);
        Ok(())
    }

    pub fn align(&mut self, kind: luxifer_core::Align) -> Result<(), AppError> {
        self.require_selection("Ausrichten")?;
        self.state.align_selection(kind);
        Ok(())
    }

    pub fn distribute(&mut self, kind: luxifer_core::Distribute) -> Result<(), AppError> {
        if !self.state.can_distribute() {
            return Err(AppError::new(
                "three_units_required",
                "Zum Verteilen müssen mindestens drei Objekte oder Gruppen ausgewählt sein.",
            ));
        }
        self.state.distribute_selection(kind);
        Ok(())
    }

    pub fn group(&mut self) -> Result<(), AppError> {
        if self.state.selected.len() < 2 {
            return Err(AppError::new(
                "two_shapes_required",
                "Zum Gruppieren müssen mindestens zwei Objekte ausgewählt sein.",
            ));
        }
        self.state.group_selected();
        Ok(())
    }

    pub fn ungroup(&mut self) -> Result<(), AppError> {
        self.require_selection("Gruppierung aufheben")?;
        self.state.ungroup_selected();
        Ok(())
    }

    pub fn nest(&mut self, gap: f64) -> Result<(), AppError> {
        if self.state.selected.len() < 2 {
            return Err(AppError::new(
                "two_shapes_required",
                "Zum Packen müssen mindestens zwei Objekte ausgewählt sein.",
            ));
        }
        self.state.nest_selected(gap);
        Ok(())
    }

    pub fn nest_fill(&mut self, gap: f64) -> Result<(), AppError> {
        self.require_selection("Bett füllen")?;
        self.state.nest_fill_selected(gap);
        Ok(())
    }

    pub fn insert_coasters(&mut self, round: bool) {
        self.state.insert_coasters(round);
    }

    pub fn boolean(&mut self, op: luxifer_core::BoolOp) -> Result<(), AppError> {
        if self.state.selected.len() < 2 {
            return Err(AppError::new(
                "two_shapes_required",
                "Für eine boolesche Operation werden mindestens zwei Objekte benötigt.",
            ));
        }
        self.state.boolean_selected(op);
        Ok(())
    }

    /// Haltesteg (v3-Modell): trennt alle Konturen, die die Steg-Linie
    /// `p0`→`p1` kreuzen, im Band der Breite `width` auf und verbindet die
    /// Schnittkanten quer (ein Core-Undo). Kreuzt die Linie nichts, kommt ein
    /// stabiler Fehler ohne Mutation.
    pub fn bridge(&mut self, p0: (f64, f64), p1: (f64, f64), width: f64) -> Result<(), AppError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(AppError::new(
                "bridge_width",
                "Die Steg-Breite muss größer als 0 mm sein.",
            ));
        }
        if !self.state.bridge_stroke(p0, p1, width) {
            return Err(AppError::new(
                "bridge_no_hit",
                "Die Steg-Linie kreuzt keine Kontur.",
            ));
        }
        Ok(())
    }

    pub fn offset(&mut self, distance: f64) -> Result<(), AppError> {
        self.require_selection("Offset")?;
        self.state.offset_selected(distance);
        Ok(())
    }

    pub fn fillet(&mut self, radius: f64) -> Result<(), AppError> {
        self.require_selection("Ecken verrunden")?;
        self.state.fillet_selected(radius);
        Ok(())
    }

    /// Muster-Füllung der Auswahl (eigener Layer, ein Core-Undo). Ungültige
    /// Parameter und eine Auswahl ohne geschlossene Konturen liefern einen
    /// stabilen Fehler ohne Mutation.
    pub fn pattern_fill(
        &mut self,
        params: &luxifer_core::pattern_fill::FillParams,
    ) -> Result<(), AppError> {
        use luxifer_core::pattern_fill::Pattern;
        self.require_selection("Muster-Füllung")?;
        let gap_ok = |g: f64| g.is_finite() && g > 0.0;
        if !gap_ok(params.gap_x) || !gap_ok(params.gap_y) {
            return Err(AppError::new(
                "pattern_gap",
                "Die Rasterabstände müssen größer als 0 mm sein.",
            ));
        }
        // Die Elementgröße zählt nur bei Formen-Mustern; Linien haben keine.
        if params.pattern != Pattern::Lines && !(params.size.is_finite() && params.size > 0.0) {
            return Err(AppError::new(
                "pattern_size",
                "Die Elementgröße muss größer als 0 mm sein.",
            ));
        }
        if !params.angle_deg.is_finite() {
            return Err(AppError::new("pattern_angle", "Der Winkel ist ungültig."));
        }
        // Der Core füllt nur geschlossene Konturen und mutiert sonst nichts —
        // das machen wir als Fehler sichtbar, statt still nichts zu tun.
        let before = self.state.shapes.len();
        self.state.pattern_fill_selected(params);
        if self.state.shapes.len() == before {
            return Err(AppError::new(
                "pattern_no_closed",
                "Die Auswahl enthält keine geschlossene Kontur — nichts zu füllen.",
            ));
        }
        Ok(())
    }
}
