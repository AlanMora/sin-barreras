//! Endpoint `/metrics` en formato Prometheus/OpenMetrics.
//!
//! No se publica por Traefik: Prometheus lo consulta dentro de la red interna.

use std::{future::Future, sync::Arc};

use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use prometheus_client::{encoding::text::encode, registry::Registry};

/// Algo que sabe refrescar sus métricas calculadas al momento de la consulta
/// (backlogs, retrasos) y entregar su registro.
pub trait MetricsSource: Send + Sync + 'static {
    fn refresh(&self) -> impl Future<Output = ()> + Send;
    fn registry(&self) -> &Registry;
}

pub fn router<S: MetricsSource>(source: Arc<S>) -> Router {
    Router::new()
        .route("/metrics", get(handler::<S>))
        .with_state(source)
}

async fn handler<S: MetricsSource>(State(source): State<Arc<S>>) -> impl IntoResponse {
    source.refresh().await;
    let mut body = String::new();
    match encode(&mut body, source.registry()) {
        Ok(()) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                "application/openmetrics-text; version=1.0.0; charset=utf-8",
            )],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
