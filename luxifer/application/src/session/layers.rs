use crate::AppError;

use super::{EditorSession, LayerToggle};

impl EditorSession {
    pub fn toggle_layer(&mut self, index: usize, toggle: LayerToggle) -> Result<(), AppError> {
        if index >= self.state.layers.len() {
            return Err(Self::invalid_layer(index));
        }
        self.state.push_undo();
        let layer = &mut self.state.layers[index];
        match toggle {
            LayerToggle::Visible => layer.visible = !layer.visible,
            LayerToggle::Enabled => layer.enabled = !layer.enabled,
            LayerToggle::Locked => layer.locked = !layer.locked,
            LayerToggle::AirAssist => layer.air_assist = !layer.air_assist,
        }
        self.state.dirty = true;
        Ok(())
    }

    pub fn move_layer(&mut self, from: usize, to: usize) -> Result<(), AppError> {
        let count = self.state.layers.len();
        if from >= count {
            return Err(Self::invalid_layer(from));
        }
        if to >= count {
            return Err(Self::invalid_layer(to));
        }
        self.state.move_layer(from, to);
        Ok(())
    }

    fn invalid_layer(index: usize) -> AppError {
        AppError::new(
            "layer_not_found",
            format!("Ebene mit Index {index} wurde nicht gefunden."),
        )
    }
}
