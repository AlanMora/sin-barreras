use c5_mock::{app, config::Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    service_kit::health::run_cli_probe_if_requested("C5_MOCK_LISTEN_ADDR", 8090);
    let config = Config::from_env()?;

    service_kit::telemetry::init("c5-mock")?;
    tracing::warn!("SIMULADOR DEL C5: la API expuesta es inventada y solo sirve para desarrollo");
    if config.api_key.is_empty() {
        tracing::warn!("C5_MOCK_API_KEY vacío: el simulador no exige autenticación");
    }

    let listen_addr = config.listen_addr;
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    tracing::info!(addr = %listen_addr, "c5-mock escuchando");
    axum::serve(listener, app(config))
        .with_graceful_shutdown(service_kit::shutdown::signal())
        .await?;
    tracing::info!("c5-mock detenido");
    Ok(())
}
