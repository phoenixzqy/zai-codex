use anyhow::Result;
use app_test_support::TestAppServer;
use codex_app_server_protocol::RequestId;
use codex_state::LogQuery;
use codex_state::SqliteConfig;
use codex_state::StateRuntime;
use codex_utils_absolute_path::test_support::PathExt;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;
use test_case::test_case;
use tokio::time::Duration;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;

#[test_case("otlp-http", 200; "http_success")]
#[test_case("otlp-http", 500; "http_failure")]
#[test_case("otlp-grpc", 200; "grpc_success")]
#[test_case("otlp-grpc", 503; "grpc_failure")]
#[tokio::test]
async fn sqlite_logs_remain_local_with_configured_metrics_exporters(
    exporter: &str,
    status: u16,
) -> Result<()> {
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status))
        .expect(/*r*/ 0)
        .mount(&collector)
        .await;
    let codex_home = TempDir::new()?;
    let protocol = if exporter == "otlp-http" {
        "protocol = \"json\"\n"
    } else {
        ""
    };
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            "[analytics]\nenabled = true\n[otel.metrics_exporter.{exporter}]\nendpoint = {:?}\n{protocol}",
            collector.uri()
        ),
    )?;
    let mut app_server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .with_env_overrides(&[
            ("OTEL_METRIC_EXPORT_INTERVAL", Some("200")),
            (
                codex_app_server_transport::REMOTE_CONTROL_DISABLED_ENV_VAR,
                Some("1"),
            ),
        ])
        .build_initialized_with_timeout(Duration::from_secs(/*secs*/ 30))
        .await?;
    let barrier = "local-log-without-metrics-export";
    app_server
        .send_response(RequestId::String(barrier.into()), json!({}))
        .await?;
    let state = StateRuntime::init(
        SqliteConfig::new_for_testing(codex_home.path().abs()),
        "test-provider".to_owned(),
    )
    .await?;
    timeout(Duration::from_secs(/*secs*/ 30), async {
        loop {
            let logs = state.query_logs(&LogQuery::default()).await?;
            if format!("{logs:?}").contains(barrier) {
                break Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(/*millis*/ 50)).await;
        }
    })
    .await??;
    tokio::time::sleep(Duration::from_millis(/*millis*/ 500)).await;
    assert_eq!(
        collector
            .received_requests()
            .await
            .expect("recorded requests")
            .len(),
        0,
    );
    state.close().await;
    Ok(())
}
