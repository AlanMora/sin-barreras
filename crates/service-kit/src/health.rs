//! Endpoints `/health` (liveness) y `/ready` (readiness).
//!
//! - `/health`: el proceso está vivo. No consulta dependencias; si fallara por
//!   una dependencia caída, Swarm reiniciaría contenedores sanos en cascada.
//! - `/ready`: el servicio puede atender tráfico (dependencias accesibles).
//!
//! Ninguno de los dos debe exponerse a internet a través de Traefik.

use std::{
    collections::BTreeMap,
    future::Future,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Serialize;

/// Resultado de revisar una dependencia.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Failing { reason: String },
}

/// Lo que cada servicio implementa para declarar sus dependencias.
pub trait ReadinessProbe: Clone + Send + Sync + 'static {
    /// Revisa cada dependencia y devuelve su estado por nombre.
    fn check(&self) -> impl Future<Output = BTreeMap<&'static str, CheckStatus>> + Send;
}

/// Sonda para servicios sin dependencias externas.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoDependencies;

impl ReadinessProbe for NoDependencies {
    async fn check(&self) -> BTreeMap<&'static str, CheckStatus> {
        BTreeMap::new()
    }
}

#[derive(Debug, Serialize)]
struct HealthBody {
    status: &'static str,
    version: &'static str,
}

#[derive(Debug, Serialize)]
struct ReadyBody {
    status: &'static str,
    checks: BTreeMap<&'static str, CheckStatus>,
}

/// Router con `/health` y `/ready`, listo para `merge` en la app.
pub fn router<P: ReadinessProbe>(probe: P, version: &'static str) -> Router {
    Router::new()
        .route(
            "/health",
            get(move || async move {
                Json(HealthBody {
                    status: "ok",
                    version,
                })
            }),
        )
        .route("/ready", get(ready::<P>))
        .with_state(probe)
}

async fn ready<P: ReadinessProbe>(State(probe): State<P>) -> (StatusCode, Json<ReadyBody>) {
    let checks = probe.check().await;
    let all_ok = checks.values().all(|c| *c == CheckStatus::Ok);
    let (code, status) = if all_ok {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not_ready")
    };
    (code, Json(ReadyBody { status, checks }))
}

/// Consulta `/health` del propio proceso y devuelve si respondió 200.
///
/// Las imágenes distroless no traen `curl` ni `wget`; por eso cada binario
/// acepta el subcomando `healthcheck`, que usa esta función, y el
/// `HEALTHCHECK` de Docker invoca al propio binario. Es síncrona a propósito:
/// no necesita levantar un runtime de tokio.
pub fn probe(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let timeout = Duration::from_secs(2);
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, timeout) else {
        return false;
    };
    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
    {
        return false;
    }
    let request = b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    if stream.write_all(request).is_err() {
        return false;
    }
    let mut status_line = [0_u8; 12];
    stream.read_exact(&mut status_line).is_ok() && status_line.ends_with(b" 200")
}

/// Si el proceso se invocó como `<binario> healthcheck`, ejecuta la sonda y
/// termina el proceso con código 0 (sano) o 1 (no sano). Si no, no hace nada.
///
/// Solo necesita la variable de la dirección de escucha (`listen_var`): la
/// sonda no debe fallar porque otra parte de la configuración sea inválida.
pub fn run_cli_probe_if_requested(listen_var: &str, default_port: u16) {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        let port = std::env::var(listen_var)
            .ok()
            .and_then(|addr| addr.parse::<SocketAddr>().ok())
            .map_or(default_port, |addr| addr.port());
        std::process::exit(if probe(port) { 0 } else { 1 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct Broken;

    impl ReadinessProbe for Broken {
        async fn check(&self) -> BTreeMap<&'static str, CheckStatus> {
            BTreeMap::from([(
                "valkey",
                CheckStatus::Failing {
                    reason: "timeout".into(),
                },
            )])
        }
    }

    async fn get_status(app: Router, path: &str) -> (StatusCode, serde_json::Value) {
        let request = Request::get(path).body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn health_is_ok_without_dependencies() {
        let (status, body) = get_status(router(NoDependencies, "1.0.0"), "/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], "1.0.0");
    }

    #[tokio::test]
    async fn ready_is_ok_without_dependencies() {
        let (status, body) = get_status(router(NoDependencies, "1.0.0"), "/ready").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
    }

    #[tokio::test]
    async fn ready_fails_when_a_dependency_fails() {
        let (status, body) = get_status(router(Broken, "1.0.0"), "/ready").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["checks"]["valkey"]["status"], "failing");
    }

    #[tokio::test]
    async fn probe_detects_a_running_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, router(NoDependencies, "1.0.0"))
                .await
                .unwrap();
        });
        let healthy = tokio::task::spawn_blocking(move || probe(port))
            .await
            .unwrap();
        assert!(healthy);
    }

    #[tokio::test]
    async fn probe_fails_when_nothing_listens() {
        // Se reserva un puerto y se libera para que nadie escuche en él.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let healthy = tokio::task::spawn_blocking(move || probe(port))
            .await
            .unwrap();
        assert!(!healthy);
    }

    #[tokio::test]
    async fn health_stays_ok_when_a_dependency_fails() {
        let (status, _) = get_status(router(Broken, "1.0.0"), "/health").await;
        assert_eq!(status, StatusCode::OK);
    }
}
