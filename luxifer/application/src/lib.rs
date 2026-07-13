//! UI-unabhängige Anwendungsschicht von LuxiFer.
//!
//! Diese Schicht besitzt die laufende Editor-Sitzung und koordiniert
//! vollständige Anwendungsfälle. Sie kennt weder egui/winit/wgpu noch Tauri.

mod assets;
mod charon;
mod error;
mod laser;
mod project;
mod session;
mod sync_inbox;
mod sync_outbox;
#[cfg(test)]
mod test_env;

pub use charon::{
    acquire_lease, connect_charon, heartbeat_lease, list_workplace_backups, release_lease,
    sync_assets, sync_project_revisions, upload_pending_revisions, upload_workplace_backups,
    wait_for_project_event, CharonBackupKind, CharonConnection, CharonHandshake, CharonLease,
    CharonProjectEvent, CharonSyncReport, CharonWorkplace, CharonWorkplaceBackup, LeaseUsage,
};
pub use error::AppError;
pub use laser::LaserService;
pub use luxifer_driver_ruida::{RuidaMachineSetting, RuidaSettingUnit};
pub use project::{ProjectDetail, ProjectService};
pub use session::{BoxShape, EditorSession, LayerParams, LayerToggle, PointPath};
pub use sync_inbox::{
    accept_inbox_revision, apply_inbox_revision, compare_inbox_revision, defer_inbox_revision,
    ignore_inbox_revision, list_inbox, reconsider_inbox_revision, InboxComparison, InboxEntry,
    InboxStatus,
};
pub use sync_outbox::{list_outbox, OutboxEntry, OutboxStatus};
