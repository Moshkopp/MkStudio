//! LuxiFer-Core — Datenmodell, Geometrie, Layer/Farbe und Undo.
//!
//! UI-frei und vollständig testbar. Einzige Quelle der Wahrheit für Geometrie
//! und Editor-Zustand; die Tauri-GUI (Svelte) und Charon bauen darauf auf.
//!
//! Aufbau (siehe docs/referenz/):
//! - [`geometry`]: reine 2D-Geometrie in mm (bbox, hit_test, translate, scale).
//! - [`model`]: [`Shape`] und [`Layer`] (Farbe = Layer).
//! - [`state`]: [`AppState`] mit Undo/Redo und dem automatischen Farbe=Layer-Modell.
//! - [`job`]: geräteunabhängiger [`JobPlan`] + [`MachineDriver`]-Trait (ADR 0001);
//!   Treiber (Ruida, GRBL, …) sind eigene Crates.
//! - [`ui_settings`]: auflösungsunabhängiges Panel-Layout, Theming und
//!   Arbeitsplatz-Settings (ADR 0002); von der GUI genutzt, aber UI-frei.

pub mod arrange;
pub mod geometry;
pub mod interact;
pub mod job;
pub mod model;
pub mod project;
pub mod scanline;
pub mod shapes;
pub mod state;
pub mod ui_settings;

pub use arrange::{Align, Distribute};
pub use geometry::{Axis, BBox, Geo, Pt};
pub use interact::Handle;
pub use job::{JobLayer, JobPlan, LayerWork, MachineDriver, Path};
pub use model::{Layer, LayerMode, Shape, SWATCH_COLORS};
pub use project::{ProjectFile, ProjectInfo};
pub use scanline::FillSegment;
pub use shapes::{PolyShape, ShapeInfo};
pub use state::AppState;
pub use ui_settings::{
    PanelKind, PanelPlacement, PanelRect, Tab, TabLayout, Theme, ThemeColor, UiSettings,
};
