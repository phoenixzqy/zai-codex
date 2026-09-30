//! Compatibility API for analytics-free builds. No events are collected or delivered.

use crate::events::AppServerRpcTransport;
use crate::events::GuardianReviewAnalyticsResult;
use crate::events::GuardianReviewTrackContext;
use crate::facts::AnalyticsJsonRpcError;
use crate::facts::AppInvocation;
use crate::facts::ArtifactOperation;
use crate::facts::CodexGoalEvent;
use crate::facts::ElicitationType;
use crate::facts::ExternalAgentConfigImportCompletedInput;
use crate::facts::ExternalAgentConfigImportFailureInput;
use crate::facts::HookRunFact;
use crate::facts::ImagePreparationFact;
use crate::facts::McpToolCallElicitation;
use crate::facts::PluginInstallRequested;
use crate::facts::PluginInstallSource;
use crate::facts::PluginMeasurementsInput;
use crate::facts::SkillInvocation;
use crate::facts::SubAgentThreadStartedInput;
use crate::facts::TrackEventsContext;
use crate::facts::TurnCodexErrorFact;
use crate::facts::TurnProfileFact;
use crate::facts::TurnResolvedConfigFact;
use crate::facts::TurnTokenUsageFact;
use crate::guardian_v2::GuardianV2Event;
use crate::product_attribution::ThreadProductUpdate;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ClientResponsePayload;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ServerResponse;
use codex_login::AuthManager;
use codex_plugin::PluginTelemetryMetadata;
use codex_protocol::ThreadId;
use codex_protocol::items::CollabAgentToolCallItem;
use codex_protocol::protocol::Event;
use codex_protocol::request_permissions::RequestPermissionsResponse;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct AnalyticsEventsClient;

impl AnalyticsEventsClient {
    pub fn update_thread_product_sku(&self, thread_id: ThreadId, update: ThreadProductUpdate) {
        let _ = (thread_id, update);
    }

    pub fn new(
        auth_manager: Arc<AuthManager>,
        base_url: String,
        analytics_enabled: Option<bool>,
    ) -> Self {
        let _ = (auth_manager, base_url, analytics_enabled);
        Self
    }

    pub fn disabled() -> Self {
        Self
    }

    pub async fn flush(&self) {}

    pub fn is_enabled(&self) -> bool {
        false
    }

    pub fn track_plugin_measurements(&self, input: PluginMeasurementsInput) {
        let _ = (input,);
    }

    pub fn track_skill_invocations(
        &self,
        tracking: TrackEventsContext,
        invocations: Vec<SkillInvocation>,
    ) {
        let _ = (tracking, invocations);
    }

    pub fn track_artifact_operation(
        &self,
        tracking: TrackEventsContext,
        operation: ArtifactOperation,
    ) {
        let _ = (tracking, operation);
    }

    pub fn track_initialize(
        &self,
        connection_id: u64,
        params: InitializeParams,
        product_client_id: String,
        rpc_transport: AppServerRpcTransport,
    ) {
        let _ = (connection_id, params, product_client_id, rpc_transport);
    }

    pub fn track_subagent_thread_started(&self, input: SubAgentThreadStartedInput) {
        let _ = (input,);
    }

    pub fn track_guardian_session_event(&self, thread_id: ThreadId, event: &Event) {
        let _ = (thread_id, event);
    }

    pub fn track_collab_tool_call(
        &self,
        turn_id: String,
        item: CollabAgentToolCallItem,
        started_at_ms: i64,
        completed_at_ms: i64,
    ) {
        let _ = (turn_id, item, started_at_ms, completed_at_ms);
    }

    pub fn track_code_mode_tool_call(&self, input: crate::facts::CodeModeToolCallFact) {
        let _ = (input,);
    }

    pub fn track_control_tool_call(&self, input: crate::facts::ControlToolCallFact) {
        let _ = (input,);
    }

    pub fn track_guardian_review(
        &self,
        tracking: &GuardianReviewTrackContext,
        result: GuardianReviewAnalyticsResult,
        completed_at_ms: u64,
    ) {
        let _ = (tracking, result, completed_at_ms);
    }

    pub fn track_app_mentioned(&self, tracking: TrackEventsContext, mentions: Vec<AppInvocation>) {
        let _ = (tracking, mentions);
    }

    pub fn track_request(
        &self,
        connection_id: u64,
        request_id: RequestId,
        request: &ClientRequest,
    ) {
        let _ = (connection_id, request_id, request);
    }

    pub fn track_app_used(
        &self,
        tracking: TrackEventsContext,
        app: AppInvocation,
        elicitation_type: Option<ElicitationType>,
    ) {
        let _ = (tracking, app, elicitation_type);
    }

    pub fn track_mcp_tool_call_elicitation(&self, input: McpToolCallElicitation) {
        let _ = (input,);
    }

    pub fn track_hook_run(&self, tracking: TrackEventsContext, hook: HookRunFact) {
        let _ = (tracking, hook);
    }

    pub fn track_plugin_used(&self, tracking: TrackEventsContext, plugin: PluginTelemetryMetadata) {
        let _ = (tracking, plugin);
    }

    pub fn track_plugin_install_requested(
        &self,
        tracking: TrackEventsContext,
        request: PluginInstallRequested,
    ) {
        let _ = (tracking, request);
    }

    pub fn track_compaction(&self, event: crate::facts::CodexCompactionEvent) {
        let _ = (event,);
    }

    pub fn track_guardian_v2_event(&self, event: GuardianV2Event) {
        let _ = (event,);
    }

    pub fn track_goal_event(&self, event: CodexGoalEvent) {
        let _ = (event,);
    }

    pub fn track_thread_hint_status(&self, event: crate::thread_hint::ThreadHintStatusEvent) {
        let _ = (event,);
    }

    pub fn track_image_preparation(&self, fact: ImagePreparationFact) {
        let _ = (fact,);
    }

    pub fn track_turn_resolved_config(&self, fact: TurnResolvedConfigFact) {
        let _ = (fact,);
    }

    pub fn track_turn_token_usage(&self, fact: TurnTokenUsageFact) {
        let _ = (fact,);
    }

    pub fn track_turn_profile(&self, fact: TurnProfileFact) {
        let _ = (fact,);
    }

    pub fn track_turn_codex_error(&self, fact: TurnCodexErrorFact) {
        let _ = (fact,);
    }

    pub fn track_plugin_installed(&self, plugin: PluginTelemetryMetadata) {
        let _ = (plugin,);
    }

    pub fn track_plugin_install_failed(
        &self,
        plugin: PluginTelemetryMetadata,
        source: PluginInstallSource,
        error_type: String,
        sub_error_type: Option<String>,
    ) {
        let _ = (plugin, source, error_type, sub_error_type);
    }

    pub fn track_external_agent_config_import_completed(
        &self,
        input: ExternalAgentConfigImportCompletedInput,
    ) {
        let _ = (input,);
    }

    pub fn track_external_agent_config_import_failure(
        &self,
        input: ExternalAgentConfigImportFailureInput,
    ) {
        let _ = (input,);
    }

    pub fn track_plugin_uninstalled(&self, plugin: PluginTelemetryMetadata) {
        let _ = (plugin,);
    }

    pub fn track_plugin_enabled(&self, plugin: PluginTelemetryMetadata) {
        let _ = (plugin,);
    }

    pub fn track_plugin_disabled(&self, plugin: PluginTelemetryMetadata) {
        let _ = (plugin,);
    }

    pub fn track_response(
        &self,
        connection_id: u64,
        request_id: RequestId,
        response: &ClientResponsePayload,
    ) {
        let _ = (connection_id, request_id, response);
    }

    pub fn track_response_with_thread_originator(
        &self,
        connection_id: u64,
        request_id: RequestId,
        response: &ClientResponsePayload,
        thread_originator: String,
    ) {
        let _ = (connection_id, request_id, response, thread_originator);
    }

    pub fn track_error_response(
        &self,
        connection_id: u64,
        request_id: RequestId,
        error: JSONRPCErrorError,
        error_type: Option<AnalyticsJsonRpcError>,
    ) {
        let _ = (connection_id, request_id, error, error_type);
    }

    pub fn track_server_request(&self, connection_id: u64, request: ServerRequest) {
        let _ = (connection_id, request);
    }

    pub fn track_server_response(&self, completed_at_ms: u64, response: ServerResponse) {
        let _ = (completed_at_ms, response);
    }

    pub fn track_effective_permissions_approval_response(
        &self,
        completed_at_ms: u64,
        request_id: RequestId,
        response: RequestPermissionsResponse,
    ) {
        let _ = (completed_at_ms, request_id, response);
    }

    pub fn track_server_request_aborted(&self, completed_at_ms: u64, request_id: RequestId) {
        let _ = (completed_at_ms, request_id);
    }

    pub fn track_notification(&self, notification: &ServerNotification) {
        let _ = (notification,);
    }
}
