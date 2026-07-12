use super::EditorSession;

impl EditorSession {
    pub fn select_at(&mut self, x: f64, y: f64, tolerance: f64, additive: bool) -> Option<usize> {
        let hit = self.state.hit_test(x, y, tolerance);
        match hit {
            Some(index) if additive => {
                if let Some(position) = self.state.selected.iter().position(|&item| item == index) {
                    self.state.selected.remove(position);
                } else {
                    self.state.selected.push(index);
                }
            }
            Some(index) if !self.state.selected.contains(&index) => {
                self.state.selected = vec![index];
            }
            None if !additive => self.state.selected.clear(),
            _ => {}
        }
        self.state.expand_selection_to_groups();
        hit
    }

    pub fn select_rect(&mut self, start: [f64; 2], end: [f64; 2]) {
        self.state
            .select_in_rect(start[0], start[1], end[0], end[1]);
        self.state.expand_selection_to_groups();
    }

    pub fn clear_selection(&mut self) {
        self.state.selected.clear();
    }

    /// Beginnt eine zusammenhängende direkte Manipulation. Beliebig viele
    /// Zwischenstände bilden danach genau einen Undo-Schritt.
    pub fn begin_edit(&mut self) {
        if self.edit_start.is_none() {
            self.edit_start = Some(self.state.clone());
            self.state.push_undo();
        }
    }

    pub fn edit_active(&self) -> bool {
        self.edit_start.is_some()
    }

    pub fn translate_edit(&mut self, dx: f64, dy: f64) {
        debug_assert!(self.edit_active(), "translate_edit ohne begin_edit");
        self.state.translate_selected(dx, dy);
    }

    pub fn scale_edit(&mut self, start: luxifer_core::BBox, target: luxifer_core::BBox) {
        debug_assert!(self.edit_active(), "scale_edit ohne begin_edit");
        self.state.scale_selection_to(start, target);
    }

    pub fn rotate_edit(&mut self, degrees: f64) {
        debug_assert!(self.edit_active(), "rotate_edit ohne begin_edit");
        self.state.rotate_selection(degrees);
    }

    pub fn commit_edit(&mut self) {
        if self.edit_start.take().is_some() {
            self.state.discard_last_undo_if_no_change();
        }
    }

    pub fn cancel_edit(&mut self) -> bool {
        let Some(start) = self.edit_start.take() else {
            return false;
        };
        self.state = start;
        true
    }
}
