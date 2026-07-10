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
pub mod assets;
pub mod dither;
pub mod geometry;
pub mod interact;
pub mod job;
pub mod laser;
pub mod model;
pub mod preview;
pub mod project;
pub mod raster;
pub mod scanline;
pub mod shapes;
pub mod state;
pub mod ui_settings;

pub use arrange::{Align, Distribute};
pub use assets::{
    apply_params, asset_meta, asset_path, assets_dir, import_image, load_asset, load_asset_luma,
    rendered_png, AssetId, AssetMeta,
};
pub use geometry::{Axis, BBox, Geo, ImageMode, ImageParams, Pt};
pub use interact::Handle;
pub use job::{
    Anchor, DriverError, JobLayer, JobParams, JobPlan, LayerWork, MachineDriver, MachineStatus,
    Path, StartMode,
};
pub use laser::{
    Connection, DriverKind, JobAction, LaserProfile, LaserRegistry, ScanOffsetCal, ScanOffsetPoint,
};
pub use model::{Layer, LayerMode, Shape, SWATCH_COLORS};
pub use project::{
    delete_project, list_projects, projects_dir, rename_project, version_thumb_path, ProjectFile,
    ProjectInfo, VersionInfo,
};
pub use raster::{raster_rows, raster_texture, Placement, RasterImage, RasterRow, RasterTexture};
pub use scanline::FillSegment;
pub use shapes::{PolyShape, ShapeInfo};
pub use state::AppState;
pub use ui_settings::{
    PanelKind, PanelPlacement, PanelRect, Tab, TabLayout, Theme, ThemeColor, UiSettings,
};
