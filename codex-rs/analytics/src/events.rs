use crate::now_unix_millis;
use codex_protocol::approvals::NetworkApprovalProtocol;
use codex_protocol::models::AdditionalPermissionProfile;
use codex_protocol::models::SandboxPermissions;
use codex_protocol::protocol::GuardianAssessmentOutcome;
use codex_protocol::protocol::GuardianCommandSource;
use codex_protocol::protocol::GuardianRiskLevel;
use codex_protocol::protocol::GuardianUserAuthorization;
use codex_protocol::protocol::TokenUsage;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppServerRpcTransport {
    Stdio,
    Websocket,
    InProcess,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianReviewDecision {
    Approved,
    Denied,
    Aborted,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianReviewTerminalStatus {
    Approved,
    Denied,
    Aborted,
    TimedOut,
    FailedClosed,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianReviewFailureReason {
    StaleAuthorization,
    Timeout,
    Cancelled,
    PromptBuildError,
    SessionError,
    ParseError,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianReviewSessionKind {
    TrunkNew,
    TrunkReused,
    EphemeralForked,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianApprovalRequestSource {
    /// Approval requested directly by the main Codex turn.
    MainTurn,
    /// Approval requested by a delegated subagent and routed through the parent
    /// session for guardian review.
    DelegatedSubagent,
}

/// Path-free permission metadata for a Guardian reviewed action.
#[derive(Clone, Debug, Serialize)]
pub struct GuardianAdditionalPermissions {
    network: Option<GuardianNetworkPermissions>,
}

#[derive(Clone, Debug, Serialize)]
struct GuardianNetworkPermissions {
    enabled: Option<bool>,
}

impl From<&AdditionalPermissionProfile> for GuardianAdditionalPermissions {
    fn from(permissions: &AdditionalPermissionProfile) -> Self {
        Self {
            network: permissions
                .network
                .as_ref()
                .map(|network| GuardianNetworkPermissions {
                    enabled: network.enabled,
                }),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GuardianReviewedAction {
    Shell {
        sandbox_permissions: SandboxPermissions,
        additional_permissions: Option<GuardianAdditionalPermissions>,
    },
    UnifiedExec {
        sandbox_permissions: SandboxPermissions,
        additional_permissions: Option<GuardianAdditionalPermissions>,
        tty: bool,
    },
    WriteStdin {
        tty: bool,
    },
    Execve {
        source: GuardianCommandSource,
        additional_permissions: Option<GuardianAdditionalPermissions>,
    },
    ApplyPatch {},
    NetworkAccess {
        protocol: NetworkApprovalProtocol,
        port: u16,
    },
    McpToolCall {
        server: String,
        tool_name: String,
        connector_id: Option<String>,
        connector_name: Option<String>,
        tool_title: Option<String>,
    },
    RequestPermissions {},
}

#[derive(Clone, Serialize)]
pub struct GuardianReviewEventParams {
    pub guardian_context_mode: Option<&'static str>,
    pub thread_id: String,
    pub turn_id: String,
    pub review_id: String,
    pub target_item_id: Option<String>,
    pub approval_request_source: GuardianApprovalRequestSource,
    pub reviewed_action: GuardianReviewedAction,
    pub reviewed_action_truncated: bool,
    pub decision: GuardianReviewDecision,
    pub terminal_status: GuardianReviewTerminalStatus,
    pub failure_reason: Option<GuardianReviewFailureReason>,
    pub attempt_count: i64,
    pub risk_level: Option<GuardianRiskLevel>,
    pub user_authorization: Option<GuardianUserAuthorization>,
    pub outcome: Option<GuardianAssessmentOutcome>,
    pub guardian_thread_id: Option<String>,
    pub guardian_session_kind: Option<GuardianReviewSessionKind>,
    pub guardian_model: Option<String>,
    pub guardian_reasoning_effort: Option<String>,
    pub guardian_default_review_model_id: Option<String>,
    pub guardian_catalog_contains_auto_review: Option<bool>,
    pub guardian_review_model_overridden: Option<bool>,
    pub guardian_review_model_override: Option<String>,
    pub guardian_model_provider_id: Option<String>,
    pub had_prior_review_context: Option<bool>,
    pub review_timeout_ms: u64,
    pub tool_call_count: Option<u64>,
    pub time_to_first_token_ms: Option<u64>,
    pub completion_latency_ms: Option<u64>,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub cache_write_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}

pub struct GuardianReviewTrackContext {
    pub started_at_ms: u64,
}

impl GuardianReviewTrackContext {
    pub fn new(
        thread_id: String,
        turn_id: String,
        review_id: String,
        target_item_id: Option<String>,
        approval_request_source: GuardianApprovalRequestSource,
        reviewed_action: GuardianReviewedAction,
        review_timeout_ms: u64,
    ) -> Self {
        let _ = (
            thread_id,
            turn_id,
            review_id,
            target_item_id,
            approval_request_source,
            reviewed_action,
            review_timeout_ms,
        );
        Self {
            started_at_ms: now_unix_millis(),
        }
    }
}

#[derive(Debug)]
pub struct GuardianReviewAnalyticsResult {
    pub guardian_context_mode: Option<&'static str>,
    pub decision: GuardianReviewDecision,
    pub terminal_status: GuardianReviewTerminalStatus,
    pub failure_reason: Option<GuardianReviewFailureReason>,
    pub attempt_count: i64,
    pub risk_level: Option<GuardianRiskLevel>,
    pub user_authorization: Option<GuardianUserAuthorization>,
    pub outcome: Option<GuardianAssessmentOutcome>,
    pub guardian_thread_id: Option<String>,
    pub guardian_session_kind: Option<GuardianReviewSessionKind>,
    pub guardian_model: Option<String>,
    pub guardian_reasoning_effort: Option<String>,
    pub guardian_default_review_model_id: Option<String>,
    pub guardian_catalog_contains_auto_review: Option<bool>,
    pub guardian_review_model_overridden: Option<bool>,
    pub guardian_review_model_override: Option<String>,
    pub guardian_model_provider_id: Option<String>,
    pub had_prior_review_context: Option<bool>,
    pub reviewed_action_truncated: bool,
    pub token_usage: Option<TokenUsage>,
    pub time_to_first_token_ms: Option<u64>,
}

impl GuardianReviewAnalyticsResult {
    pub fn without_session() -> Self {
        Self {
            guardian_context_mode: None,
            decision: GuardianReviewDecision::Denied,
            terminal_status: GuardianReviewTerminalStatus::FailedClosed,
            failure_reason: None,
            attempt_count: 1,
            risk_level: None,
            user_authorization: None,
            outcome: None,
            guardian_thread_id: None,
            guardian_session_kind: None,
            guardian_model: None,
            guardian_reasoning_effort: None,
            guardian_default_review_model_id: None,
            guardian_catalog_contains_auto_review: None,
            guardian_review_model_overridden: None,
            guardian_review_model_override: None,
            guardian_model_provider_id: None,
            had_prior_review_context: None,
            reviewed_action_truncated: false,
            token_usage: None,
            time_to_first_token_ms: None,
        }
    }

    pub fn from_session(params: GuardianReviewSessionAnalyticsParams) -> Self {
        Self {
            guardian_thread_id: Some(params.guardian_thread_id),
            guardian_session_kind: Some(params.guardian_session_kind),
            guardian_model: Some(params.guardian_model),
            guardian_reasoning_effort: params.guardian_reasoning_effort,
            guardian_default_review_model_id: Some(params.guardian_default_review_model_id),
            guardian_catalog_contains_auto_review: Some(
                params.guardian_catalog_contains_auto_review,
            ),
            guardian_review_model_overridden: Some(params.guardian_review_model_overridden),
            guardian_review_model_override: params.guardian_review_model_override,
            guardian_model_provider_id: Some(params.guardian_model_provider_id),
            had_prior_review_context: Some(params.had_prior_review_context),
            ..Self::without_session()
        }
    }
}

pub struct GuardianReviewSessionAnalyticsParams {
    pub guardian_thread_id: String,
    pub guardian_session_kind: GuardianReviewSessionKind,
    pub guardian_model: String,
    pub guardian_reasoning_effort: Option<String>,
    pub guardian_default_review_model_id: String,
    pub guardian_catalog_contains_auto_review: bool,
    pub guardian_review_model_overridden: bool,
    pub guardian_review_model_override: Option<String>,
    pub guardian_model_provider_id: String,
    pub had_prior_review_context: bool,
}
