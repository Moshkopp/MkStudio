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

    pub fn mirror(&mut self, axis: studio_core::Axis) -> Result<(), AppError> {
        self.require_selection("Spiegeln")?;
        self.state.mirror_selection(axis);
        Ok(())
    }

    pub fn align(&mut self, kind: studio_core::Align) -> Result<(), AppError> {
        self.require_selection("Ausrichten")?;
        self.state.align_selection(kind);
        Ok(())
    }

    pub fn distribute(&mut self, kind: studio_core::Distribute) -> Result<(), AppError> {
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

    pub fn boolean(&mut self, op: studio_core::BoolOp) -> Result<(), AppError> {
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
        if !distance.is_finite() || distance.abs() < 0.001 {
            return Err(AppError::new(
                "offset_distance",
                "Der Offset-Abstand muss mindestens 0,001 mm betragen.",
            ));
        }
        let before = self.state.shapes.len();
        self.state.offset_selected(distance);
        if self.state.shapes.len() == before {
            return Err(AppError::new(
                "offset_empty",
                "Für diese Kontur und diesen Abstand konnte kein Offset erzeugt werden.",
            ));
        }
        Ok(())
    }

    /// Berechnet die spaeter einzufuegenden Offset-Konturen ohne Mutation,
    /// Dirty-State oder Undo. Fuer die Live-Vorschau des Canvas-Werkzeugs.
    pub fn offset_preview(&self, distance: f64) -> Result<Vec<studio_core::Shape>, AppError> {
        self.require_selection("Offset")?;
        if !distance.is_finite() || distance.abs() < 0.001 {
            return Err(AppError::new(
                "offset_distance",
                "Der Offset-Abstand muss mindestens 0,001 mm betragen.",
            ));
        }
        let mut preview = self.state.clone();
        let before = preview.shapes.len();
        preview.offset_selected(distance);
        if preview.shapes.len() == before {
            return Err(AppError::new(
                "offset_empty",
                "Für diese Kontur und diesen Abstand konnte kein Offset erzeugt werden.",
            ));
        }
        Ok(preview
            .selected
            .iter()
            .filter_map(|&index| preview.shapes.get(index).cloned())
            .collect())
    }

    pub fn fillet(&mut self, radius: f64) -> Result<(), AppError> {
        self.require_selection("Ecken verrunden")?;
        self.state.fillet_selected(radius);
        Ok(())
    }

    /// Berechnet eine Fillet-Vorschau für eine Kontur, ohne Session, Undo oder
    /// Dirty-State zu verändern. Jeder Punktindex trägt seinen eigenen Radius.
    pub fn fillet_preview(
        &self,
        shape_index: usize,
        radii: &[(usize, f64)],
    ) -> Result<(studio_core::Shape, usize), AppError> {
        if radii
            .iter()
            .any(|(_, radius)| !radius.is_finite() || *radius <= 0.0)
        {
            return Err(AppError::new(
                "fillet_radius",
                "Der Radius muss größer als 0 mm sein.",
            ));
        }
        let mut preview = self.state.clone();
        let accepted = preview.fillet_shape_corner_radii(shape_index, radii);
        let shape = preview.shapes.get(shape_index).cloned().ok_or_else(|| {
            AppError::new(
                "fillet_shape",
                "Die gewählte Kontur ist nicht mehr vorhanden.",
            )
        })?;
        Ok((shape, accepted))
    }

    /// Übernimmt alle gewählten Fillets gemeinsam als einen Undo-Schritt.
    pub fn fillet_corners(
        &mut self,
        shape_index: usize,
        radii: &[(usize, f64)],
    ) -> Result<usize, AppError> {
        if radii.is_empty() {
            return Err(AppError::new(
                "fillet_empty",
                "Bitte mindestens eine Ecke auswählen.",
            ));
        }
        if radii
            .iter()
            .any(|(_, radius)| !radius.is_finite() || *radius <= 0.0)
        {
            return Err(AppError::new(
                "fillet_radius",
                "Der Radius muss größer als 0 mm sein.",
            ));
        }
        let accepted = self.state.fillet_shape_corner_radii(shape_index, radii);
        if accepted == 0 {
            return Err(AppError::new(
                "fillet_rejected",
                "Keine gewählte Ecke kann mit diesem Radius verrundet werden.",
            ));
        }
        Ok(accepted)
    }

    /// Muster-Füllung der Auswahl (eigener Layer, ein Core-Undo). Ungültige
    /// Parameter und eine Auswahl ohne geschlossene Konturen liefern einen
    /// stabilen Fehler ohne Mutation.
    pub fn pattern_fill(
        &mut self,
        params: &studio_core::pattern_fill::FillParams,
    ) -> Result<(), AppError> {
        use studio_core::pattern_fill::Pattern;
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
