//! Señal de apagado ordenado (SIGTERM de Docker/Swarm o Ctrl+C).

/// Se resuelve cuando el proceso recibe SIGTERM o SIGINT.
///
/// Swarm envía SIGTERM y espera `stop_grace_period` antes de matar el
/// contenedor; los servicios deben drenar conexiones dentro de ese margen.
pub async fn signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "no se pudo escuchar Ctrl+C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "no se pudo escuchar SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("señal de apagado recibida; drenando");
}
