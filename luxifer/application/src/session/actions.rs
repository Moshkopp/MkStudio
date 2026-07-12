use crate::AppError;

use super::EditorSession;

impl EditorSession {
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
}
