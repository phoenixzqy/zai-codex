//! Foreground updates honor app-server consent and never claim unconfirmed installation.

use anyhow::Context as _;
use pretty_assertions::assert_eq;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;

#[tokio::test]
async fn foreground_update_respects_consent_and_reports_unconfirmed() -> anyhow::Result<()> {
    for (analytics_config, analytics_default_enabled, expected_requests) in [
        ("", false, 0),
        ("", true, 0),
        ("analytics.enabled = true\n", false, 0),
        ("analytics.enabled = false\n", true, 0),
    ] {
        let codex = codex_utils_cargo_bin::cargo_bin("codex")?;
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::path("/metrics"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let home = tempfile::tempdir()?;
        std::fs::write(
            home.path().join("config.toml"),
            format!(
                "cli_auth_credentials_store = \"file\"\n{analytics_config}[otel]\nmetrics_exporter = {{ otlp-http = {{ endpoint = \"{}/metrics\", protocol = \"json\" }} }}\n",
                server.uri(),
            ),
        )?;
        // No managed installation exists, so the command cannot confirm an applied update.
        let mut command = tokio::process::Command::new(&codex);
        command
            .current_dir(home.path())
            .env("CODEX_HOME", home.path())
            .env_remove(codex_app_server_daemon::telemetry::HANDOFF_ENV)
            .arg("app-server");
        if analytics_default_enabled {
            command.arg("--analytics-default-enabled");
        }
        let output = command
            .args(["daemon", "update", "--from-cli", "--yes"])
            .output()
            .await?;
        assert!(!output.status.success());
        let requests = server
            .received_requests()
            .await
            .context("metric requests")?;
        assert_eq!(requests.len(), expected_requests);
    }
    Ok(())
}
