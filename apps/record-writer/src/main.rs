use std::sync::Arc;

use record_writer::{config::Config, consumer::Consumer};
use registry::writer::Writer;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    service_kit::health::run_cli_probe_if_requested("RECORD_WRITER_LISTEN_ADDR", 8081);
    let config = Config::from_env()?;
    let _telemetry = service_kit::telemetry::init("record-writer")?;

    let client = valkey_client::connect_with_retry(&config.valkey).await?;
    // Pool perezoso: si Postgres está caído, el proceso arranca igual y los
    // eventos esperan en el stream hasta que vuelva.
    let writer = Writer::new(config.db.pool(4));
    let consumer = Arc::new(Consumer {
        client,
        writer,
        consumer: config.consumer.clone(),
        tuning: config.tuning,
        stats: Default::default(),
    });
    let (stop_tx, stop_rx) = watch::channel(false);
    let tasks = record_writer::spawn(&consumer, &stop_rx);

    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!(addr = %config.listen_addr, consumer = %config.consumer, "record-writer escuchando");
    axum::serve(listener, record_writer::router(Arc::clone(&consumer)))
        .with_graceful_shutdown(service_kit::shutdown::signal())
        .await?;

    // Drenado: se termina el lote en curso; lo no confirmado queda pendiente.
    let _ = stop_tx.send(true);
    for task in tasks {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), task).await;
    }
    tracing::info!("record-writer detenido");
    Ok(())
}
