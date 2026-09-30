use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use codex_otel::MetricsClient;
use codex_otel::MetricsConfig;
use codex_otel::OtelExporter;
use codex_otel::OtelHttpProtocol;
use codex_otel::OtelProvider;
use codex_otel::OtelSettings;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::error::Error;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::PathBuf;

#[test]
fn configured_exporters_cannot_start_remote_delivery() -> Result<(), Box<dyn Error>> {
    let server = TcpListener::bind("127.0.0.1:0")?;
    server.set_nonblocking(true)?;
    let exporter = OtelExporter::OtlpHttp {
        endpoint: format!("http://{}", server.local_addr()?),
        headers: HashMap::new(),
        protocol: OtelHttpProtocol::Json,
        tls: None,
    };
    let settings = OtelSettings {
        http_client_factory: HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
        environment: "test".to_string(),
        service_name: "zai-codex".to_string(),
        service_version: env!("CARGO_PKG_VERSION").to_string(),
        codex_home: PathBuf::from("."),
        exporter: exporter.clone(),
        trace_exporter: exporter.clone(),
        metrics_exporter: exporter.clone(),
        runtime_metrics: true,
        span_attributes: BTreeMap::new(),
        tracestate: BTreeMap::new(),
    };
    assert!(OtelProvider::try_new(&settings)?.is_none());
    assert!(
        MetricsClient::new(MetricsConfig::otlp("test", "zai-codex", "0.0.0", exporter)).is_err()
    );
    assert_eq!(server.accept().unwrap_err().kind(), ErrorKind::WouldBlock);
    Ok(())
}
