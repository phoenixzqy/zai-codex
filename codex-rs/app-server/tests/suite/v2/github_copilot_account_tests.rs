use super::CreateConfigTomlParams;
use super::DEFAULT_READ_TIMEOUT;
use super::create_config_toml;
use anyhow::Result;
use app_test_support::TestAppServer;
use codex_app_server_protocol::Account;
use codex_app_server_protocol::GetAccountParams;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use codex_login::AuthCredentialsStoreMode;
use codex_login::AuthKeyringBackendKind;
use codex_login::GitHubCopilotAuth;
use codex_login::login_with_github_copilot;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn github_copilot_account_is_visible_without_openai_backend_requests() -> Result<()> {
    let server = responses::start_mock_server().await;
    let codex_home = TempDir::new()?;
    create_config_toml(
        codex_home.path(),
        CreateConfigTomlParams {
            base_url: Some(server.uri()),
            chatgpt_base_url: Some(server.uri()),
            requires_openai_auth: Some(true),
            ..Default::default()
        },
    )?;
    login_with_github_copilot(
        codex_home.path(),
        GitHubCopilotAuth::new(
            "github-test-token".to_string(),
            "https://api.individual.githubcopilot.com".to_string(),
            Some("octocat".to_string()),
            Some("copilot_individual".to_string()),
            vec!["gpt-5.5".to_string()],
        )?,
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    )?;
    let mut app_server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let request_id = app_server
        .send_get_account_request(GetAccountParams {
            refresh_token: true,
        })
        .await?;
    let account: GetAccountResponse =
        timeout(DEFAULT_READ_TIMEOUT, app_server.read_response(request_id)).await??;
    assert_eq!(
        account,
        GetAccountResponse {
            account: Some(Account::GitHubCopilot {
                login: Some("octocat".to_string()),
                copilot_sku: Some("copilot_individual".to_string()),
            }),
            requires_openai_auth: true,
            workspace_routing: None,
        }
    );
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        0
    );
    Ok(())
}

#[tokio::test]
async fn account_feedback_upload_stays_disabled_when_configuration_enables_it() -> Result<()> {
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), CreateConfigTomlParams::default())?;
    let config_path = codex_home.path().join("config.toml");
    let config = std::fs::read_to_string(&config_path)?;
    std::fs::write(
        config_path,
        format!("{config}\n[feedback]\nenabled = true\n"),
    )?;
    let mut app_server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let request_id = app_server
        .send_request(
            "feedback/upload",
            Some(json!({"classification": "bug", "includeLogs": true})),
        )
        .await?;
    let error = timeout(
        DEFAULT_READ_TIMEOUT,
        app_server.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    assert_eq!(
        error.error,
        JSONRPCErrorError {
            code: -32600,
            message: "sending feedback is disabled by configuration".to_string(),
            data: None,
        }
    );
    Ok(())
}
