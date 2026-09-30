//! Guardian V2 classifier and fast-decision facts, enriched by the existing reducer.
//! Payloads contain attribution and bounded outcomes, never prompts or tool arguments.

use serde::Serialize;

#[derive(Serialize)]
pub struct GuardianV2Event {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: Option<String>,
    pub model: Option<String>,
    pub occurred_at_ms: u64,
    #[serde(flatten)]
    pub kind: GuardianV2EventKind,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum GuardianV2EventKind {
    Classification {
        guardian_context_mode: &'static str,
        outcome: &'static str,
        risk_level: Option<&'static str>,
        duration_ms: u64,
    },
    FastDecision {
        decision: &'static str,
    },
}
