//! Logs estructurados y trazas OpenTelemetry.
//!
//! POLÍTICA: los logs y las trazas NUNCA deben contener el contenido de
//! mensajes, ubicaciones ni IPs. Esos datos solo viven cifrados en Postgres.
//! Registra identificadores (sesión, incidente, evento), nunca cargas útiles.
//!
//! Trazas: si existe `OTEL_EXPORTER_OTLP_ENDPOINT` (p. ej.
//! `http://otel-collector:4318`), se exportan por OTLP/HTTP al colector. Sin
//! esa variable no se exporta nada y el servicio funciona igual.

use opentelemetry::{
    Context, global,
    propagation::{Extractor, TextMapPropagator},
    trace::{TraceContextExt, TracerProvider as _},
};
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace::SdkTracerProvider};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{EnvFilter, Layer, fmt, prelude::*};

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("no se pudo inicializar el subscriber de tracing: {0}")]
    Init(String),
    #[error("no se pudo crear el exportador OTLP: {0}")]
    Exporter(String),
}

/// Formato de salida de los logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// JSON de una línea por evento (producción).
    Json,
    /// Texto legible (desarrollo local).
    Pretty,
}

impl LogFormat {
    /// Lee `LOG_FORMAT` (`json` | `pretty`); por defecto `json`.
    pub fn from_env() -> Self {
        match std::env::var("LOG_FORMAT").as_deref() {
            Ok("pretty") => Self::Pretty,
            _ => Self::Json,
        }
    }
}

/// Mantener vivo mientras corra el servicio: al soltarse, envía las trazas pendientes.
#[must_use]
pub struct TelemetryGuard {
    provider: Option<SdkTracerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            let _ = provider.shutdown();
        }
    }
}

/// Inicializa logs y, si está configurado, la exportación de trazas.
/// El nivel se controla con `RUST_LOG` (por defecto `info`).
pub fn init(service_name: &'static str) -> Result<TelemetryGuard, TelemetryError> {
    global::set_text_map_propagator(TraceContextPropagator::new());
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let provider = match std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        Ok(_) => {
            // El exportador lee el endpoint de las variables OTEL_* estándar.
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .build()
                .map_err(|e| TelemetryError::Exporter(e.to_string()))?;
            let provider = SdkTracerProvider::builder()
                .with_resource(Resource::builder().with_service_name(service_name).build())
                .with_batch_exporter(exporter)
                .build();
            global::set_tracer_provider(provider.clone());
            Some(provider)
        }
        Err(_) => None,
    };
    let otel_layer = provider.as_ref().map(|p| {
        tracing_opentelemetry::layer()
            .with_tracer(p.tracer(service_name))
            .boxed()
    });

    let fmt_layer = match LogFormat::from_env() {
        LogFormat::Json => fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .boxed(),
        LogFormat::Pretty => fmt::layer().boxed(),
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(otel_layer)
        .try_init()
        .map_err(|e| TelemetryError::Init(e.to_string()))?;

    tracing::info!(
        service = service_name,
        version = env!("CARGO_PKG_VERSION"),
        traces = provider.is_some(),
        "telemetría lista"
    );
    Ok(TelemetryGuard { provider })
}

struct One<'a>(&'a str);

impl Extractor for One<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        (key == "traceparent").then_some(self.0)
    }
    fn keys(&self) -> Vec<&str> {
        vec!["traceparent"]
    }
}

/// Hace que `span` continúe la traza indicada por un `traceparent` W3C (el que
/// genera la PWA), para tener trazas de punta a punta PWA → BFF → C5.
pub fn continue_trace(span: &tracing::Span, traceparent: Option<&str>) {
    if let Some(tp) = traceparent {
        let cx = TraceContextPropagator::new().extract(&One(tp));
        if cx.span().span_context().is_valid() {
            let _ = span.set_parent(cx);
        }
    }
}

/// `traceparent` del span actual, para propagarlo al C5.
pub fn current_traceparent() -> Option<String> {
    let cx: Context = tracing::Span::current().context();
    let span = cx.span();
    let sc = span.span_context();
    sc.is_valid().then(|| {
        format!(
            "00-{}-{}-{:02x}",
            sc.trace_id(),
            sc.span_id(),
            sc.trace_flags().to_u8()
        )
    })
}
