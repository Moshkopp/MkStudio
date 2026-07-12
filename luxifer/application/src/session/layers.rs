use luxifer_core::LayerMode;

use crate::AppError;

use super::{EditorSession, LayerToggle};

/// UI-unabhängiger Parametersatz eines Layers für den Bearbeiten-Dialog.
///
/// Bewusst keine Kopie des Tauri-DTOs: `mode` ist der typisierte Core-Enum
/// (kein String), und der Bereich fachlich gültiger Werte wird beim Anwenden
/// validiert, nicht erst im UI. Native hält davon nur einen kurzlebigen
/// Entwurf; die Wahrheit lebt im `AppState`.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerParams {
    pub name: String,
    pub mode: LayerMode,
    pub speed_mm_s: f64,
    pub power_pct: f64,
    pub min_power_pct: f64,
    pub passes: u32,
    pub air_assist: bool,
    pub line_step_mm: f64,
    pub dpi: f64,
    pub bidirectional: bool,
}

impl LayerParams {
    /// Übernimmt die aktuellen Werte eines Layers als Startpunkt für die
    /// Bearbeitung.
    pub fn from_layer(layer: &luxifer_core::Layer) -> Self {
        Self {
            name: layer.name.clone(),
            mode: layer.mode,
            speed_mm_s: layer.speed_mm_s,
            power_pct: layer.power_pct,
            min_power_pct: layer.min_power_pct,
            passes: layer.passes,
            air_assist: layer.air_assist,
            line_step_mm: layer.line_step_mm,
            dpi: layer.dpi,
            bidirectional: layer.bidirectional,
        }
    }
}

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

    /// Setzt alle Parameter eines Layers in genau einem Undo-Schritt.
    ///
    /// Validierung erfolgt vollständig vor jeder Mutation: Bei ungültigen Werten
    /// bleibt der Zustand unangetastet (kein Dirty, kein Undo-Eintrag). Ein
    /// Image-Layer darf seinen Modus nicht verlieren, und ein Nicht-Image-Layer
    /// darf nicht unbeabsichtigt zum Image-Layer werden (er hätte kein Asset).
    pub fn set_layer_params(&mut self, index: usize, params: LayerParams) -> Result<(), AppError> {
        let layer = self
            .state
            .layers
            .get(index)
            .ok_or_else(|| Self::invalid_layer(index))?;

        Self::validate_layer_params(&params, layer.mode)?;

        self.state.push_undo();
        let layer = &mut self.state.layers[index];
        layer.name = params.name;
        layer.mode = params.mode;
        layer.speed_mm_s = params.speed_mm_s;
        layer.power_pct = params.power_pct;
        layer.min_power_pct = params.min_power_pct;
        layer.passes = params.passes;
        layer.air_assist = params.air_assist;
        layer.line_step_mm = params.line_step_mm;
        layer.dpi = params.dpi;
        layer.bidirectional = params.bidirectional;
        self.state.dirty = true;
        Ok(())
    }

    /// Fachliche Wertegrenzen. `current_mode` ist der bisherige Layer-Modus und
    /// entscheidet die Image-Invariante.
    fn validate_layer_params(
        params: &LayerParams,
        current_mode: LayerMode,
    ) -> Result<(), AppError> {
        let is_image = current_mode == LayerMode::Image;
        if is_image && params.mode != LayerMode::Image {
            return Err(AppError::new(
                "image_layer_mode",
                "Ein Bild-Layer kann nicht in einen Vektor-Layer umgewandelt werden.",
            ));
        }
        if !is_image && params.mode == LayerMode::Image {
            return Err(AppError::new(
                "image_layer_mode",
                "Nur ein importiertes Bild kann einen Image-Layer erzeugen.",
            ));
        }

        if !in_percent(params.power_pct) {
            return Err(AppError::new(
                "power_range",
                "Die maximale Leistung muss zwischen 0 und 100 % liegen.",
            ));
        }
        if !in_percent(params.min_power_pct) {
            return Err(AppError::new(
                "power_range",
                "Die minimale Leistung muss zwischen 0 und 100 % liegen.",
            ));
        }
        if params.min_power_pct > params.power_pct {
            return Err(AppError::new(
                "power_order",
                "Die minimale Leistung darf die maximale Leistung nicht übersteigen.",
            ));
        }
        // `is_positive` ist bewusst so formuliert, dass NaN als ungültig gilt:
        // eine `<= 0.0`-Prüfung ließe NaN fälschlich durch.
        if !is_positive(params.speed_mm_s) {
            return Err(AppError::new(
                "speed_invalid",
                "Die Geschwindigkeit muss größer als 0 mm/s sein.",
            ));
        }
        if params.passes < 1 {
            return Err(AppError::new(
                "passes_invalid",
                "Es muss mindestens ein Durchlauf ausgeführt werden.",
            ));
        }
        if !is_positive(params.line_step_mm) {
            return Err(AppError::new(
                "line_step_invalid",
                "Der Zeilenabstand muss größer als 0 mm sein.",
            ));
        }
        if !is_positive(params.dpi) {
            return Err(AppError::new(
                "dpi_invalid",
                "Die DPI müssen größer als 0 sein.",
            ));
        }
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

/// Endlich und echt größer als 0 (NaN gilt als ungültig).
fn is_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

/// Endlicher Prozentwert im Bereich [0, 100] (NaN gilt als ungültig).
fn in_percent(value: f64) -> bool {
    value.is_finite() && (0.0..=100.0).contains(&value)
}
