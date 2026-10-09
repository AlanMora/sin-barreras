//! `sb-admin investigate`: consulta de una sesión para una investigación,
//! queja o solicitud legal.
//!
//! 1. Solo con el rol `sb_investigator`, que únicamente puede llamar funciones
//!    que registran el acceso en `registry.access_log` (quién, motivo, qué).
//! 2. Verifica la integridad de la cadena de la sesión con las mismas filas
//!    recibidas: si algo fue alterado, lo dice ANTES de mostrar el contenido.
//! 3. Descifra con la llave PRIVADA de investigación, que vive fuera de línea.

use anyhow::{Context, bail};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use crypto::RecipientPrivateKey;
use events::{
    EventKind, KeyRef, RecordEvent, SealedPayload, SensitivePayload, WrappedKeyData,
    open::open_sealed,
};
use protocol::Timestamp;
use registry::{db::DbConfig, verify::ChainChecker};
use serde_json::{Value, json};
use service_kit::config::secret;
use sqlx::{Row, postgres::PgRow};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct Request {
    pub session: Option<Uuid>,
    pub c5_incident: Option<String>,
    pub requested_by: String,
    pub reason: String,
    pub json: bool,
}

pub fn parse(args: &[&str]) -> anyhow::Result<Request> {
    let get = |flag: &str| {
        args.windows(2)
            .find(|w| w[0] == flag)
            .map(|w| w[1].to_owned())
    };
    let requested_by = get("--requested-by").context("falta --requested-by \"Nombre y cargo\"")?;
    let reason = get("--reason").context("falta --reason \"folio o motivo\"")?;
    let session = get("--session")
        .map(|s| s.parse())
        .transpose()
        .context("--session no es un UUID")?;
    let c5_incident = get("--c5");
    if session.is_none() && c5_incident.is_none() {
        bail!("indica --session <UUID> o --c5 <id del C5>");
    }
    Ok(Request {
        session,
        c5_incident,
        requested_by,
        reason,
        json: args.contains(&"--json"),
    })
}

pub async fn run(request: Request) -> anyhow::Result<()> {
    let pool = DbConfig::from_env("sb-admin-investigate")?.pool(1);
    let session = match (request.session, &request.c5_incident) {
        (Some(s), _) => s,
        (None, Some(c5)) => {
            let found: Vec<(Uuid, Option<Uuid>, OffsetDateTime, OffsetDateTime)> =
                sqlx::query_as("SELECT * FROM investigation.find_sessions($1, $2, $3)")
                    .bind(c5)
                    .bind(&request.requested_by)
                    .bind(&request.reason)
                    .fetch_all(&pool)
                    .await?;
            match found.as_slice() {
                [] => bail!("no hay sesiones con el incidente del C5 {c5}"),
                [(session, ..)] => *session,
                many => {
                    println!(
                        "El incidente {c5} tiene {} sesiones; indica una con --session:",
                        many.len()
                    );
                    for (s, i, first, last) in many {
                        println!(
                            "  {s}  incidente={}  {first} → {last}",
                            i.map(|i| i.to_string()).unwrap_or_default()
                        );
                    }
                    return Ok(());
                }
            }
        }
        (None, None) => unreachable!("validado en parse"),
    };

    let rows = sqlx::query("SELECT * FROM investigation.read_session($1, $2, $3)")
        .bind(session)
        .bind(&request.requested_by)
        .bind(&request.reason)
        .fetch_all(&pool)
        .await?;
    if rows.is_empty() {
        bail!("la sesión {session} no tiene registros");
    }
    let mut checker = ChainChecker::default();
    for row in &rows {
        checker.check(row)?;
    }

    let keys: Vec<(String, String, Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT recipient_key_id, alg, encapped, wrapped_key FROM investigation.session_keys($1, $2, $3)",
    )
    .bind(session)
    .bind(&request.requested_by)
    .bind(&request.reason)
    .fetch_all(&pool)
    .await?;
    let privates = load_private_keys()?;

    let mut timeline = Vec::with_capacity(rows.len());
    for row in &rows {
        timeline.push(entry(row, session, &keys, &privates)?);
    }

    if request.json {
        let report = json!({
            "session_id": session,
            "requested_by": request.requested_by,
            "reason": request.reason,
            "integrity": { "ok": checker.issues.is_empty(), "records": checker.records, "issues": checker.issues },
            "events": timeline,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("══════════════════════════════════════════════════════════════════");
    println!(" INVESTIGACIÓN · Sin Barreras · DOCUMENTO CONFIDENCIAL");
    println!("══════════════════════════════════════════════════════════════════");
    println!(" Sesión:          {session}");
    println!(" Solicitado por:  {}", request.requested_by);
    println!(" Motivo / folio:  {}", request.reason);
    println!(" Generado:        {}", Timestamp::now());
    println!(" Este acceso quedó registrado en la bitácora de accesos.");
    if checker.issues.is_empty() {
        println!(
            " Integridad:      ✓ verificada ({} registros, cadena completa e inalterada)",
            checker.records
        );
    } else {
        println!(
            " Integridad:      ✗ {} PROBLEMAS — el contenido podría estar alterado:",
            checker.issues.len()
        );
        for issue in &checker.issues {
            println!("                  {}", serde_json::to_string(issue)?);
        }
    }
    println!("──────────────────────────────────────────────────────────────────");
    for item in &timeline {
        let when = item["occurred_at"].as_str().unwrap_or_default();
        let kind = item["kind"].as_str().unwrap_or_default();
        let content = match &item["content"] {
            Value::Null => String::new(),
            c => describe(c),
        };
        let ids = item["c5_incident_id"]
            .as_str()
            .map(|c| format!(" [C5 {c}]"))
            .unwrap_or_default();
        println!(" {when}  {kind:<26}{content}{ids}");
    }
    Ok(())
}

fn load_private_keys() -> anyhow::Result<Vec<RecipientPrivateKey>> {
    let raw = secret("RECORD_PRIVATE_KEYS")
        .context("falta RECORD_PRIVATE_KEYS(_FILE): la llave privada de investigación")?;
    raw.split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| RecipientPrivateKey::parse(s).map_err(Into::into))
        .collect()
}

fn entry(
    row: &PgRow,
    session: Uuid,
    keys: &[(String, String, Vec<u8>, Vec<u8>)],
    privates: &[RecipientPrivateKey],
) -> anyhow::Result<Value> {
    let kind_text: String = row.try_get("kind")?;
    let occurred_at: OffsetDateTime = row.try_get("occurred_at")?;
    let attributes: String = row.try_get("attributes_json")?;
    let mut item = json!({
        "occurred_at": Timestamp::from(occurred_at).to_string(),
        "kind": kind_text,
        "attributes": serde_json::from_str::<Value>(&attributes)?,
        "c5_incident_id": row.try_get::<Option<String>, _>("c5_incident_id")?,
        "content": Value::Null,
    });
    let Some(ciphertext) = row.try_get::<Option<Vec<u8>>, _>("sealed_ciphertext")? else {
        return Ok(item);
    };
    let kind: EventKind = serde_json::from_value(Value::String(kind_text.clone()))?;
    let mut event = RecordEvent::new(kind, "");
    event.event_id = row.try_get("event_id")?;
    event.session_id = Some(session);
    let key_ref: KeyRef = serde_json::from_str(&row.try_get::<String, _>("sealed_key_ref")?)?;
    let sealed = SealedPayload {
        alg: row.try_get("sealed_alg")?,
        key_ref,
        nonce: B64.encode(row.try_get::<Vec<u8>, _>("sealed_nonce")?),
        ciphertext: B64.encode(ciphertext),
    };
    let session_key = keys.iter().find_map(|(recipient, alg, encapped, wrapped)| {
        privates
            .iter()
            .any(|p| &p.key_id == recipient)
            .then(|| WrappedKeyData {
                alg: alg.clone(),
                recipient_key_id: recipient.clone(),
                encapped: B64.encode(encapped),
                ciphertext: B64.encode(wrapped),
            })
    });
    let recipient_id = match (&sealed.key_ref, &session_key) {
        (KeyRef::Inline { wrapped }, _) => wrapped.recipient_key_id.clone(),
        (KeyRef::Session { .. }, Some(k)) => k.recipient_key_id.clone(),
        (KeyRef::Session { .. }, None) => {
            item["content"] = json!({ "error": "no hay llave privada para esta sesión (¿borrado criptográfico o llave rotada?)" });
            return Ok(item);
        }
    };
    let Some(private) = privates.iter().find(|p| p.key_id == recipient_id) else {
        item["content"] = json!({ "error": format!("falta la llave privada {recipient_id}") });
        return Ok(item);
    };
    item["content"] = match open_sealed(&event, &sealed, session_key.as_ref(), private) {
        Ok(payload) => serde_json::to_value(payload)?,
        Err(e) => json!({ "error": format!("no se pudo descifrar: {e}") }),
    };
    Ok(item)
}

fn describe(content: &Value) -> String {
    match serde_json::from_value::<SensitivePayload>(content.clone()) {
        Ok(SensitivePayload::Message { text }) => format!("“{text}”"),
        Ok(SensitivePayload::Location {
            latitude,
            longitude,
            accuracy_m,
            source,
            ..
        }) => format!(
            "{latitude:.6}, {longitude:.6}{} ({source})",
            accuracy_m
                .map(|a| format!(" ±{a:.0} m"))
                .unwrap_or_default()
        ),
        Ok(SensitivePayload::Network { ip, user_agent }) => {
            format!(
                "IP {} · {}",
                ip.unwrap_or_default(),
                user_agent.unwrap_or_default()
            )
        }
        Ok(SensitivePayload::VideoCall { data }) => format!("videollamada {data}"),
        Err(_) => content.to_string(),
    }
}
