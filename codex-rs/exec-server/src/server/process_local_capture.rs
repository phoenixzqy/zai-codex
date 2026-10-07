use std::sync::Arc;
use std::sync::Mutex;

use opentelemetry_sdk::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde_json::Value;
use serde_json::json;
use tracing::Subscriber;
use tracing::field::Field;
use tracing::field::Visit;
use tracing_opentelemetry::OtelData;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

pub(super) struct LocalTelemetry {
    pub(super) tracer_provider: Option<SdkTracerProvider>,
    pub(super) span_exporter: InMemorySpanExporter,
}

#[derive(Clone, Default)]
pub(super) struct LocalLogs(pub(super) Arc<Mutex<Vec<Value>>>);

impl<SubscriberType: Subscriber + for<'span> LookupSpan<'span>> Layer<SubscriberType>
    for LocalLogs
{
    fn on_event(&self, event: &tracing::Event<'_>, context: Context<'_, SubscriberType>) {
        if !matches!(
            event.metadata().target(),
            "codex_otel.log_only" | "codex_otel.network_proxy"
        ) {
            return;
        }
        let mut attributes = LocalAttributes::default();
        event.record(&mut attributes);
        let mut record = json!({"attributes": attributes.0});
        if let Some(span) = context.event_span(event)
            && let Some(data) = span.extensions().get::<OtelData>()
            && let (Some(trace_id), Some(span_id)) = (data.trace_id(), data.span_id())
        {
            record["traceId"] = json!(trace_id.to_string());
            record["spanId"] = json!(span_id.to_string());
        }
        self.0.lock().expect("local logs").push(record);
    }
}

#[derive(Default)]
struct LocalAttributes(Vec<Value>);

impl Visit for LocalAttributes {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .push(json!({"key": field.name(), "value": {"stringValue": value}}));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0
            .push(json!({"key": field.name(), "value": {"intValue": value.to_string()}}));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0
            .push(json!({"key": field.name(), "value": {"intValue": value.to_string()}}));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0
            .push(json!({"key": field.name(), "value": {"boolValue": value}}));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }
}
