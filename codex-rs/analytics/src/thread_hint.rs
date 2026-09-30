//! Per-attempt thread hint status analytics without hint contents.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadHintStatus {
    Succeeded,
    Failed,
}

pub struct ThreadHintStatusEvent {
    pub thread_id: String,
    pub status: ThreadHintStatus,
    pub occurred_at_ms: u64,
}
