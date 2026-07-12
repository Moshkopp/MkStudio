//! UI-unabhängige Anwendungsschicht von LuxiFer.
//!
//! Diese Schicht besitzt die laufende Editor-Sitzung und koordiniert
//! vollständige Anwendungsfälle. Sie kennt weder egui/winit/wgpu noch Tauri.

mod error;
mod laser;
mod project;
mod session;

pub use error::AppError;
pub use laser::LaserService;
pub use project::ProjectService;
pub use session::{BoxShape, EditorSession, LayerParams, LayerToggle, PointPath};
