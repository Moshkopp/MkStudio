//! LuxiFer-Core — Datenmodell, Geometrie, Layer/Farbe und Undo.
//!
//! UI-frei und vollständig testbar. Einzige Quelle der Wahrheit für Geometrie
//! und Editor-Zustand; die Tauri-GUI (Svelte) und Charon bauen darauf auf.
//!
//! Aufbau (siehe docs/referenz/):
//! - [`geometry`]: reine 2D-Geometrie in mm (bbox, hit_test, translate, scale).
//! - [`model`]: [`Shape`] und [`Layer`] (Farbe = Layer).
//! - [`state`]: [`AppState`] mit Undo/Redo und dem automatischen Farbe=Layer-Modell.

pub mod geometry;
pub mod model;
pub mod state;

pub use geometry::{BBox, Geo, Pt};
pub use model::{Layer, LayerMode, Shape, SWATCH_COLORS};
pub use state::AppState;
