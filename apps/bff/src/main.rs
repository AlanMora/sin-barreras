use bff::{app, config::Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    service_kit::health::run_cli_probe_if_requested(config.listen_addr.port());

    service_kit::telemetry::init("bff")?;

    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!(addr = %config.listen_addr, "bff escuchando");

    axum::serve(listener, app())
        .with_graceful_shutdown(service_kit::shutdown::signal())
        .await?;
    tracing::info!("bff detenido");
    Ok(())
}
