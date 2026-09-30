//! Turn-cost enrichment is intentionally disabled.
//!
//! The former worker queried OpenAI-owned analytics endpoints after turns completed.
//! This build performs no secondary telemetry requests or remote OTLP exports.

use std::sync::Arc;

use codex_core::config::Config;
use codex_login::AuthManager;
use codex_otel::SessionTelemetry;
use codex_protocol::ThreadId;
use codex_protocol::protocol::Event;

pub(crate) struct TurnCostWorker;

#[derive(Clone)]
pub(crate) struct TurnCostWorkerHandle;

impl TurnCostWorker {
    pub(crate) fn spawn(config: Arc<Config>, auth_manager: Arc<AuthManager>) -> Option<Self> {
        let _ = (config, auth_manager);
        None
    }

    pub(crate) fn handle(&self) -> TurnCostWorkerHandle {
        TurnCostWorkerHandle
    }

    pub(crate) fn shutdown(&self) {}
}

impl TurnCostWorkerHandle {
    pub(crate) fn observe_event(
        &self,
        thread_id: ThreadId,
        thread_config: &Config,
        event: &Event,
        session_telemetry: impl FnOnce() -> SessionTelemetry,
    ) {
        let _ = (thread_id, thread_config, event, session_telemetry);
    }
}
