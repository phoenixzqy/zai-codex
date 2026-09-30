use std::path::Path;

use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::write_chatgpt_auth;
use codex_config::types::AuthCredentialsStoreMode;
use codex_config::types::OtelExporterKind;
use codex_config::types::OtelHttpProtocol;
use codex_core::config::ConfigBuilder;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[tokio::test]
async fn reporting_defaults_cannot_enable_remote_metrics() -> Result<()> {
    for default_analytics_enabled in [false, true] {
        let codex_home = TempDir::new()?;
        let mut config = ConfigBuilder::default()
            .codex_home(codex_home.path().to_path_buf())
            .build()
            .await?;
        config.otel.metrics_exporter = OtelExporterKind::OtlpHttp {
            endpoint: "http://127.0.0.1:4318/v1/metrics".to_owned(),
            headers: Default::default(),
            protocol: OtelHttpProtocol::Json,
            tls: None,
        };
        config.analytics_enabled = None;
        let provider = codex_core::otel_init::build_provider(
            &config,
            "0.0.0-test",
            Some("codex-app-server"),
            default_analytics_enabled,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        assert_eq!(provider.is_none(), true);
    }
    Ok(())
}

pub(crate) async fn mount_analytics_capture(server: &MockServer, codex_home: &Path) -> Result<()> {
    Mock::given(method("POST"))
        .and(path("/codex/analytics-events/events"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;
    write_chatgpt_auth(
        codex_home,
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;
    app_test_support::mount_workspace_routing(server).await;
    Ok(())
}
