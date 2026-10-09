#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Pruebas del registro contra Postgres 18 real (contenedor).

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use events::{EventKind, KeyRef, RecordEvent, SealedPayload, StreamRecord};
use registry::{
    checkpoint,
    db::{DbConfig, MIGRATOR, ROLES_SQL, migrator_config},
    verify,
    writer::{Entry, WriteError, Writer},
};
use sqlx::PgPool;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner},
};
use uuid::Uuid;

struct Db {
    _container: ContainerAsync<Postgres>,
    port: u16,
    superuser: PgPool,
}

impl Db {
    fn config(&self, user: &str) -> DbConfig {
        DbConfig {
            host: "127.0.0.1".into(),
            port: self.port,
            database: "postgres".into(),
            user: user.into(),
            password: if user == "postgres" {
                "postgres".into()
            } else {
                "test".into()
            },
            ca_file: None,
            role: None,
            search_path: None,
            application_name: "test".into(),
        }
    }

    fn pool(&self, user: &str) -> PgPool {
        self.config(user).pool(4)
    }
}

async fn setup() -> Db {
    let container = Postgres::default()
        .with_tag("18-alpine")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let mut db = Db {
        _container: container,
        port,
        superuser: PgPool::connect_lazy("postgres://x@localhost/x").unwrap(),
    };
    db.superuser = db.pool("postgres");
    let mut roles = ROLES_SQL.replace(":\"dbname\"", "postgres");
    for var in [
        "migrator",
        "writer",
        "auditor",
        "reporting",
        "investigator",
        "replicator",
    ] {
        roles = roles.replace(&format!(":'{var}_password'"), "'test'");
    }
    // SQL propio (archivo de roles del repositorio), no entrada de usuarios.
    sqlx::raw_sql(sqlx::AssertSqlSafe(roles))
        .execute(&db.superuser)
        .await
        .unwrap();
    let migrator = migrator_config(db.config("sb_migrator")).pool(1);
    MIGRATOR.run(&migrator).await.unwrap();
    db
}

fn event(kind: EventKind, session: Option<Uuid>) -> RecordEvent {
    let mut e = RecordEvent::new(kind, "bff/test")
        .attr("channel", "text_chat")
        .attr("n", 1);
    e.session_id = session;
    e
}

fn sealed_entry(session: Uuid, stream_id: &str) -> Entry {
    Entry {
        stream_id: stream_id.into(),
        record: StreamRecord {
            event: event(EventKind::CitizenMessage, Some(session)),
            sealed: Some(SealedPayload {
                alg: "A256GCM".into(),
                key_ref: KeyRef::Session {
                    session_id: session,
                },
                nonce: B64.encode([1_u8; 12]),
                ciphertext: B64.encode(b"texto-cifrado-de-prueba"),
            }),
        },
    }
}

fn plain_entry(kind: EventKind, session: Option<Uuid>, stream_id: &str) -> Entry {
    Entry {
        stream_id: stream_id.into(),
        record: StreamRecord {
            event: event(kind, session),
            sealed: None,
        },
    }
}

#[tokio::test]
async fn writes_chains_idempotently_and_verifies() {
    let db = setup().await;
    let writer = Writer::new(db.pool("sb_writer"));
    let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
    let batch = vec![
        plain_entry(EventKind::SessionStarted, Some(a), "1-0"),
        sealed_entry(a, "2-0"),
        plain_entry(EventKind::SessionStarted, Some(b), "3-0"),
        sealed_entry(a, "4-0"),
        plain_entry(EventKind::C5OrphanEvent, None, "5-0"),
    ];
    let outcome = writer.write_batch(&batch).await.unwrap();
    assert_eq!((outcome.inserted, outcome.duplicates), (5, 0));

    // Reintento del mismo lote (caída entre COMMIT y XACK): nada se duplica.
    let again = writer.write_batch(&batch).await.unwrap();
    assert_eq!((again.inserted, again.duplicates), (0, 5));

    let seqs: Vec<i64> = sqlx::query_scalar(
        "SELECT chain_seq FROM registry.records WHERE chain_id = $1 ORDER BY chain_seq",
    )
    .bind(a.to_string())
    .fetch_all(&db.superuser)
    .await
    .unwrap();
    assert_eq!(seqs, vec![1, 2, 3]);

    checkpoint::create(writer.pool())
        .await
        .unwrap()
        .expect("checkpoint");
    let report = verify::verify(&db.pool("sb_auditor"), None).await.unwrap();
    assert!(report.is_ok(), "{:?}", report.issues);
    assert_eq!(
        (report.records, report.chains, report.checkpoints),
        (5, 3, 1)
    );
}

#[tokio::test]
async fn database_enforces_append_only_and_least_privilege() {
    let db = setup().await;
    let writer_pool = db.pool("sb_writer");
    Writer::new(writer_pool.clone())
        .write_batch(&[sealed_entry(Uuid::now_v7(), "1-0")])
        .await
        .unwrap();

    let denied = |sql: &'static str, pool: PgPool| async move {
        let err = sqlx::query(sql).execute(&pool).await.expect_err(sql);
        err.as_database_error()
            .and_then(|e| e.code())
            .map(|c| c.to_string())
            .unwrap_or_default()
    };
    // El writer no puede modificar, borrar, vaciar ni leer el contenido.
    assert_eq!(
        denied(
            "UPDATE registry.records SET kind = 'x'",
            writer_pool.clone()
        )
        .await,
        "42501"
    );
    assert_eq!(
        denied("DELETE FROM registry.records", writer_pool.clone()).await,
        "42501"
    );
    assert_eq!(
        denied("TRUNCATE registry.records", writer_pool.clone()).await,
        "42501"
    );
    assert_eq!(
        denied(
            "SELECT sealed_ciphertext FROM registry.records",
            writer_pool.clone()
        )
        .await,
        "42501"
    );
    assert_eq!(
        denied(
            "SELECT attributes_json FROM registry.records",
            writer_pool.clone()
        )
        .await,
        "42501"
    );
    // Reportes: solo vistas sin contenido sensible.
    let reporting = db.pool("sb_reporting");
    assert_eq!(
        denied("SELECT * FROM registry.records", reporting.clone()).await,
        "42501"
    );
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM reporting.sessions")
        .fetch_one(&reporting)
        .await
        .unwrap();
    assert_eq!(sessions, 1);
    // Auditoría: solo lectura. La sesión del rol es read-only (25006) y, aun
    // sin eso, no tiene permiso (42501): dos barreras.
    let code = denied("DELETE FROM registry.records", db.pool("sb_auditor")).await;
    assert!(
        code == "25006" || code == "42501",
        "código inesperado {code}"
    );

    // Ni siquiera el dueño de las tablas puede modificar: lo impiden los triggers.
    let owner = db.superuser.clone();
    sqlx::query("SET ROLE sb_owner").execute(&owner).await.ok();
    for sql in [
        "UPDATE registry.records SET kind = 'x'",
        "DELETE FROM registry.records",
        "TRUNCATE registry.records",
    ] {
        let mut conn = owner.acquire().await.unwrap();
        sqlx::query("SET ROLE sb_owner")
            .execute(&mut *conn)
            .await
            .unwrap();
        let err = sqlx::query(sql).execute(&mut *conn).await.expect_err(sql);
        assert!(err.to_string().contains("solo inserción"), "{sql}: {err}");
    }
}

#[tokio::test]
async fn verify_detects_every_kind_of_tampering() {
    let db = setup().await;
    let writer = Writer::new(db.pool("sb_writer"));
    let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
    let mut batch = Vec::new();
    for i in 0..4 {
        batch.push(sealed_entry(a, &format!("{i}-a")));
        batch.push(sealed_entry(b, &format!("{i}-b")));
    }
    writer.write_batch(&batch).await.unwrap();
    checkpoint::create(writer.pool()).await.unwrap();
    let auditor = db.pool("sb_auditor");
    assert!(verify::verify(&auditor, None).await.unwrap().is_ok());

    // Un atacante con superusuario desactiva los triggers y altera datos.
    let su = &db.superuser;
    sqlx::query("ALTER TABLE registry.records DISABLE TRIGGER USER")
        .execute(su)
        .await
        .unwrap();

    // 1) Cambia el texto cifrado de un mensaje.
    sqlx::query("UPDATE registry.records SET sealed_ciphertext = 'alterado' WHERE chain_id = $1 AND chain_seq = 2")
        .bind(a.to_string())
        .execute(su)
        .await
        .unwrap();
    let report = verify::verify(&auditor, Some(&a.to_string()))
        .await
        .unwrap();
    assert!(
        report.issues.iter().any(|i| i.problem.contains("alterado")),
        "{:?}",
        report.issues
    );

    // 2) Borra un registro intermedio de la otra sesión.
    sqlx::query("DELETE FROM registry.records WHERE chain_id = $1 AND chain_seq = 3")
        .bind(b.to_string())
        .execute(su)
        .await
        .unwrap();
    let report = verify::verify(&auditor, Some(&b.to_string()))
        .await
        .unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.problem.contains("secuencia rota")),
        "{:?}",
        report.issues
    );
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.problem.contains("prev_hash")),
        "{:?}",
        report.issues
    );
}

#[tokio::test]
async fn checkpoints_detect_deletion_of_a_whole_session_even_if_hashes_are_recomputed() {
    let db = setup().await;
    let writer = Writer::new(db.pool("sb_writer"));
    let (keep, erase) = (Uuid::now_v7(), Uuid::now_v7());
    writer
        .write_batch(&[
            sealed_entry(keep, "1-0"),
            sealed_entry(erase, "2-0"),
            sealed_entry(erase, "3-0"),
        ])
        .await
        .unwrap();
    checkpoint::create(writer.pool()).await.unwrap();

    // El atacante borra TODA una sesión: su cadena desaparece completa y las
    // demás cadenas siguen siendo internamente válidas.
    let su = &db.superuser;
    sqlx::query("ALTER TABLE registry.records DISABLE TRIGGER USER")
        .execute(su)
        .await
        .unwrap();
    sqlx::query("DELETE FROM registry.records WHERE chain_id = $1")
        .bind(erase.to_string())
        .execute(su)
        .await
        .unwrap();

    let report = verify::verify(&db.pool("sb_auditor"), None).await.unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.checkpoint_id.is_some() && i.problem.contains("registros")),
        "{:?}",
        report.issues
    );
}

#[tokio::test]
async fn plaintext_sensitive_content_is_rejected_as_permanent() {
    let db = setup().await;
    let writer = Writer::new(db.pool("sb_writer"));
    let mut entry = plain_entry(EventKind::CitizenMessage, Some(Uuid::now_v7()), "9-0");
    entry.record.event.sensitive = Some(events::SensitivePayload::Message {
        text: "en claro".into(),
    });
    match writer.write_batch(&[entry]).await {
        Err(WriteError::Permanent { stream_id, .. }) => assert_eq!(stream_id, "9-0"),
        other => panic!("se esperaba error permanente: {other:?}"),
    }
}

#[tokio::test]
async fn session_keys_are_stored_from_their_event() {
    let db = setup().await;
    let writer = Writer::new(db.pool("sb_writer"));
    let session = Uuid::now_v7();
    let entry = Entry {
        stream_id: "1-0".into(),
        record: StreamRecord {
            event: RecordEvent::new(EventKind::SessionKeyCreated, "bff/test")
                .session(session)
                .attr("recipient_key_id", "inv-1")
                .attr("alg", crypto_alg())
                .attr("encapped", B64.encode([7_u8; 32]))
                .attr("wrapped_key", B64.encode([8_u8; 48])),
            sealed: None,
        },
    };
    writer
        .write_batch(std::slice::from_ref(&entry))
        .await
        .unwrap();
    writer.write_batch(&[entry]).await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM registry.session_keys WHERE session_id = $1")
            .bind(session)
            .fetch_one(&db.superuser)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

fn crypto_alg() -> &'static str {
    "HPKE-Base-X25519-SHA256-A256GCM"
}

#[tokio::test]
async fn investigation_requires_the_access_log() {
    let db = setup().await;
    let writer = Writer::new(db.pool("sb_writer"));
    let session = Uuid::now_v7();
    writer
        .write_batch(&[sealed_entry(session, "1-0"), sealed_entry(session, "2-0")])
        .await
        .unwrap();
    let investigator = db.pool("sb_investigator");

    // Sin acceso directo a las tablas.
    let err = sqlx::query("SELECT * FROM registry.records")
        .execute(&investigator)
        .await
        .unwrap_err();
    assert_eq!(err.as_database_error().unwrap().code().unwrap(), "42501");

    // Solo a través de la función, que deja rastro.
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT chain_seq FROM investigation.read_session($1, 'Lic. Pérez (Jurídico)', 'Queja 2026/0153')",
    )
    .bind(session)
    .fetch_all(&investigator)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    let log: Vec<(String, String, String, Option<i32>)> = sqlx::query_as(
        "SELECT requested_by, reason, action, rows_returned FROM registry.access_log ORDER BY access_id",
    )
    .fetch_all(&db.superuser)
    .await
    .unwrap();
    assert_eq!(log[0].2, "read_session");
    assert_eq!(
        log[1],
        (
            "Lic. Pérez (Jurídico)".into(),
            "Queja 2026/0153".into(),
            "read_session_rows".into(),
            Some(2)
        )
    );

    // Sin motivo no hay datos.
    let err = sqlx::query("SELECT * FROM investigation.read_session($1, 'Lic. Pérez', '')")
        .bind(session)
        .execute(&investigator)
        .await
        .unwrap_err();
    assert_eq!(err.as_database_error().unwrap().code().unwrap(), "23514");

    // La bitácora tampoco se puede borrar.
    let su = &db.superuser;
    let err = sqlx::query("DELETE FROM registry.access_log")
        .execute(su)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("solo inserción"));
}
