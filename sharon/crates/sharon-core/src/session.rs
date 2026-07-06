use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Machine Session: Es darf immer nur ein Client aktiv mit einer
/// Maschine verbunden sein. Sharon vergibt und entzieht Sessions,
/// spricht aber niemals selbst mit der Maschine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineSession {
    pub machine_id: Uuid,
    pub holder_client_id: Uuid,
    pub acquired_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    /// Ein anderer Client möchte die Maschine — aktueller Halter soll trennen.
    ReleaseRequested {
        machine_id: Uuid,
        requested_by: Uuid,
    },
    Acquired {
        machine_id: Uuid,
        client_id: Uuid,
    },
    Released {
        machine_id: Uuid,
    },
}
