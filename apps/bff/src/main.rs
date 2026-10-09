use std::{net::SocketAddr, sync::Arc};

use bff::{
    AppState, c5::mock_http::MockHttpGateway, config::Config, qr::QrVerifier, valkey::Valkey,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    service_kit::health::run_cli_probe_if_requested("BFF_LISTEN_ADDR", 8080);
    let config = Config::from_env()?;

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("sign-qr") {
        return sign_qr(&config, &args[2..]);
    }

    service_kit::telemetry::init("bff")?;
    if config.allowed_origins.is_empty() {
        tracing::warn!("ALLOWED_ORIGINS vacío: solo se aceptarán clientes sin cabecera Origin");
    }
    if config.qr_keys.is_empty() {
        tracing::warn!("QR_KEYS vacío: todos los QR quedarán como no verificados");
    }

    let valkey = Valkey::connect(&config.valkey).await?;
    let gateway = Arc::new(MockHttpGateway::new(&config.c5)?);
    let listen_addr = config.listen_addr;
    let replica = config.replica_id.clone();
    let state = AppState::new(config, valkey, gateway);
    let _background = bff::spawn_background(&state);

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    tracing::info!(addr = %listen_addr, %replica, "bff escuchando");
    axum::serve(
        listener,
        bff::router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(service_kit::shutdown::signal())
    .await?;
    tracing::info!("bff detenido");
    Ok(())
}

/// `bff sign-qr <PUNTO> [ID_LLAVE] [URL_BASE]`: imprime la URL firmada para el QR.
fn sign_qr(config: &Config, args: &[String]) -> anyhow::Result<()> {
    let point_id = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("uso: bff sign-qr <PUNTO> [ID_LLAVE] [URL_BASE]"))?;
    if !protocol::validate::is_point_id(point_id) {
        anyhow::bail!("identificador de punto inválido: {point_id} (ejemplo: TOT-0142)");
    }
    let key_id = args.get(1).map_or(bff::qr::DEFAULT_KEY_ID, String::as_str);
    let base = args
        .get(2)
        .map_or("https://sinbarreras.zapopan.gob.mx", String::as_str);
    let verifier = QrVerifier::new(config.qr_keys.clone());
    let signature = verifier
        .sign(key_id, point_id)
        .ok_or_else(|| anyhow::anyhow!("no existe la llave {key_id} en QR_KEYS"))?;
    println!("{base}/?t={point_id}&k={key_id}&s={signature}");
    Ok(())
}
