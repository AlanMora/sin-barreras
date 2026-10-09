//! `sb-admin`: administración del registro completo de Sin Barreras.
//!
//! ```text
//! sb-admin keygen <id>              Genera llaves (investigación, operativa, QR)
//! sb-admin migrate                  Aplica migraciones (rol sb_migrator)
//! sb-admin verify [--session ID] [--json]
//!                                   Verifica la integridad (rol sb_auditor)
//! sb-admin dlq list                 Lista la cola de errores del registro
//! sb-admin dlq replay <ID|--all>    Reinyecta eventos de la cola de errores
//! ```
//!
//! La conexión se configura con las mismas variables que los servicios
//! (`PG_*`, `VALKEY_*`), cada comando con el rol de mínimo privilegio que le
//! corresponde.

use std::collections::HashMap;

use anyhow::{Context, bail};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use events::{DEAD_LETTER_STREAM, RECORD_STREAM};
use fred::interfaces::StreamsInterface;
use registry::db::{DbConfig, MIGRATOR, migrator_config};
use valkey_client::ValkeyConfig;

const USAGE: &str = "uso: sb-admin <keygen ID | migrate | verify [--session ID] [--json] | dlq list | dlq replay <ID|--all>>";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["keygen", id] => keygen(id),
        ["migrate"] => migrate().await,
        ["verify", rest @ ..] => verify(rest).await,
        ["dlq", "list"] => dlq_list().await,
        ["dlq", "replay", target] => dlq_replay(target).await,
        _ => bail!(USAGE),
    }
}

/// Genera las llaves que necesita un despliegue. La PRIVADA de investigación
/// debe guardarse fuera de los servidores en línea (ver docs/llaves.md).
fn keygen(id: &str) -> anyhow::Result<()> {
    let (private, public) = crypto::generate_recipient_keypair(id);
    let random = |n: usize| -> anyhow::Result<String> {
        let mut bytes = vec![0_u8; n];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("aleatoriedad: {e}"))?;
        Ok(B64.encode(bytes))
    };
    println!("# Llave PÚBLICA de investigación (secret RECORD_PUBLIC_KEY del BFF)");
    println!("{}", public.to_spec());
    println!();
    println!("# Llave PRIVADA de investigación — NUNCA en los servidores en línea");
    println!("{}", private.to_spec().as_str());
    println!();
    println!("# Llave operativa (secret RECORD_OPERATIONAL_KEYS del BFF)");
    println!("{id}:{}", random(32)?);
    println!();
    println!("# Llave de firma de QR (secret QR_KEYS del BFF)");
    println!("{id}:{}", random(32)?);
    Ok(())
}

async fn migrate() -> anyhow::Result<()> {
    let config = migrator_config(DbConfig::from_env("sb-admin-migrate")?);
    let pool = config.pool(1);
    MIGRATOR
        .run(&pool)
        .await
        .context("no se pudieron aplicar las migraciones")?;
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sb_migrations._sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await?;
    println!("✓ migraciones al día ({applied} aplicadas)");
    Ok(())
}

async fn verify(args: &[&str]) -> anyhow::Result<()> {
    let json = args.contains(&"--json");
    let session = args.windows(2).find(|w| w[0] == "--session").map(|w| w[1]);
    let pool = DbConfig::from_env("sb-admin-verify")?.pool(2);
    let report = registry::verify::verify(&pool, session).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Registros verificados:      {}", report.records);
        println!("Cadenas (sesiones):         {}", report.chains);
        if session.is_none() {
            println!("Checkpoints verificados:    {}", report.checkpoints);
            println!(
                "Registros sin anclar aún:   {}",
                report.records_after_last_checkpoint
            );
        }
        if report.is_ok() {
            println!(
                "✓ INTEGRIDAD VERIFICADA: ningún registro fue alterado, borrado ni reordenado."
            );
        } else {
            println!(
                "✗ SE DETECTARON {} PROBLEMAS DE INTEGRIDAD:",
                report.issues.len()
            );
            for issue in &report.issues {
                println!("  - {}", serde_json::to_string(issue)?);
            }
        }
    }
    if !report.is_ok() {
        std::process::exit(2);
    }
    Ok(())
}

type Entries = Vec<(String, HashMap<String, String>)>;

async fn dlq_list() -> anyhow::Result<()> {
    let client = valkey_client::connect(&ValkeyConfig::from_env()?).await?;
    let entries: Entries = client
        .xrange_values(DEAD_LETTER_STREAM, "-", "+", None)
        .await?;
    if entries.is_empty() {
        println!("✓ La cola de errores está vacía.");
    }
    for (id, fields) in &entries {
        let get = |k: &str| fields.get(k).map_or("-", String::as_str);
        // El contenido (`e`) no se imprime: su parte sensible está cifrada y
        // basta con el id de origen para reinyectarlo.
        println!(
            "{id}  origen={}  falló={}  motivo={}",
            get("source_id"),
            get("failed_at"),
            get("reason")
        );
    }
    Ok(())
}

/// Reinyecta al stream principal (después de corregir la causa). El writer es
/// idempotente: si el evento ya se había guardado, no se duplica.
async fn dlq_replay(target: &str) -> anyhow::Result<()> {
    let client = valkey_client::connect(&ValkeyConfig::from_env()?).await?;
    let entries: Entries = if target == "--all" {
        client
            .xrange_values(DEAD_LETTER_STREAM, "-", "+", None)
            .await?
    } else {
        client
            .xrange_values(DEAD_LETTER_STREAM, target, target, None)
            .await?
    };
    if entries.is_empty() {
        bail!("no hay entradas que reinyectar");
    }
    for (id, fields) in entries {
        let raw = fields.get("e").context("entrada sin contenido")?;
        let _: String = client
            .xadd(RECORD_STREAM, false, None, "*", vec![("e", raw.as_str())])
            .await?;
        let _: i64 = client.xdel(DEAD_LETTER_STREAM, id.as_str()).await?;
        println!("↺ {id} reinyectado");
    }
    Ok(())
}
