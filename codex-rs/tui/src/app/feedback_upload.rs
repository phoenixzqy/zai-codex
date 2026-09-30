//! Reject feedback before staging diagnostics or sending app-server requests.

use codex_app_server_client::AppServerPath;
use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_protocol::FeedbackUploadParams;
use codex_app_server_protocol::FeedbackUploadResponse;
use codex_feedback::CodexFeedback;
use color_eyre::eyre::Result;
use color_eyre::eyre::eyre;

pub(super) async fn fetch_feedback_upload(
    request_handle: AppServerRequestHandle,
    codex_home: Option<AppServerPath>,
    params: FeedbackUploadParams,
    feedback: CodexFeedback,
) -> Result<FeedbackUploadResponse> {
    let _ = (request_handle, codex_home, params, feedback);
    Err(eyre!("Remote error reporting is disabled in zai-codex"))
}
