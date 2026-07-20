//! Studio-Core — Datenmodell, Geometrie, Layer/Farbe und Undo.
//!
//! UI-frei und vollständig testbar. Einzige Quelle der Wahrheit für Geometrie
//! und Editor-Zustand; die Tauri-GUI (Svelte) und Hub bauen darauf auf.
//!
//! Aufbau (siehe docs/referenz/):
//! - [`geometry`]: reine 2D-Geometrie in mm (bbox, hit_test, translate, scale).
//! - [`model`]: [`Shape`] und [`Layer`] (Farbe = Layer).
//! - [`state`]: [`AppState`] mit Undo/Redo und dem automatischen Farbe=Layer-Modell.
//! - [`job`]: geräteunabhängiger [`JobPlan`] + [`MachineDriver`]-Trait (ADR 0001);
//!   Treiber (Ruida, GRBL, …) sind eigene Crates.
//! - [`ui_settings`]: Theming und Arbeitsplatz-Settings (ADR 0002); von der GUI
//!   genutzt, aber UI-frei.

pub mod arrange;
pub mod assets;
pub mod bezier;
pub mod branding;
pub mod datetime;
pub mod dither;
pub mod execution;
pub mod geo_ops;
pub mod geometry;
pub mod import;
pub mod interact;
pub mod job;
pub mod laser;
pub mod materials;
pub mod model;
pub mod nesting;
pub mod pattern_fill;
pub mod preview;
pub mod project;
pub mod raster;
pub mod scanline;
pub mod shapes;
pub mod shortcuts;
pub mod state;
pub mod text;
pub mod trace;
pub mod trim;
pub mod ui_settings;

pub use arrange::{Align, Distribute};
pub use assets::{
    add_asset_tags, apply_params, asset_hidden, asset_meta, asset_path, asset_thumbnail,
    assets_dir, delete_asset, derive_tags, hide_asset, import_image, import_image_preserve_alpha,
    import_source, list_assets, load_asset, load_asset_luma, load_asset_luma_alpha,
    load_asset_rgba, rendered_png, store_asset, AssetId, AssetKind, AssetMeta,
};
pub use execution::{ExecutionKind, ExecutionMove, ExecutionTrace, TraceBuilder};
pub use geo_ops::BoolOp;
pub use geometry::{Axis, BBox, Geo, ImageMode, ImageParams, Pt};
pub use interact::{keep_aspect, resize_to_cursor, Handle};
pub use job::{
    Anchor, AxisDir, DriverCapabilities, DriverError, JobLayer, JobParams, JobPlan, JogMotion,
    LayerWork, MachineAxis, MachineDriver, MachineSetting, MachineSettingUnit, MachineStatus, Path,
    StartMode, StartReference,
};
pub use laser::{
    AxisConfig, BedOrigin, Connection, DriverKind, JobAction, LaserProfile, LaserRegistry,
    SavedOrigin, ScanOffsetCal, ScanOffsetPoint, LASER_PROFILE_SCHEMA_VERSION,
};
pub use materials::{MaterialLibrary, MaterialProcess, MaterialProcessDefaults, MaterialProfile};
pub use model::{Layer, LayerMode, Shape, TextMeta, SWATCH_COLORS};
pub use project::{
    data_root, delete_project, list_projects, projects_dir, rename_project, version_thumb_path,
    ProjectFile, ProjectInfo, VersionInfo,
};
pub use raster::{raster_rows, raster_texture, Placement, RasterImage, RasterRow, RasterTexture};
pub use scanline::FillSegment;
pub use shapes::{PolyShape, ShapeInfo};
pub use shortcuts::{
    ShortcutAction, ShortcutBindings, ShortcutChord, ShortcutKey, ShortcutMouseButton,
    ShortcutTrigger,
};
pub use state::AppState;
pub use ui_settings::{Theme, ThemeColor, ThemePalette, UiSettings};
