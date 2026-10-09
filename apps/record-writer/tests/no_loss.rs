#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Pruebas de NO PÉRDIDA del registro (REGLA DE ORO 2), con Valkey y Postgres
//! reales en contenedores. Cada prueba exige que lleguen EXACTAMENTE los
//! eventos publicados: ni uno menos (pérdida) ni uno más (duplicado).

use std::{sync::Arc, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use events::{
    DEAD_LETTER_STREAM, EventKind, KeyRef, RECORD_GROUP, RECORD_STREAM, RecordEvent, SealedPayload,
    StreamRecord,
};
use fred::{interfaces::StreamsInterface, prelude::Client};
use record_writer::{config::Tuning, consumer::Consumer};
use registry::{
    test_support::{TestDb, free_port},
    verify,
    writer::Writer,
};
use testcontainers_modules::{
    testcontainers::{ContainerAsync, ImageExt, core::ContainerPort, runners::AsyncRunner},
    valkey::Valkey,
};
use tokio::sync::watch;
use uuid::Uuid;
use valkey_client::ValkeyConfig;

type Fields = std::collections::HashMap<String, String>;
type RawEntries = Vec<(String, Fields)>;

struct TestValkey {
    container: ContainerAsync<Valkey>,
    url: String,
}

/// Valkey con la MISMA persistencia que producción (AOF + fsync always) y
/// puerto fijo para sobrevivir a reinicios.
async fn start_valkey() -> TestValkey {
    let port = free_port();
    let container = Valkey::default()
        .with_tag("9.1-alpine")
        .with_mapped_port(port, ContainerPort::Tcp(6379))
        .with_cmd([
            "valkey-server",
            "--appendonly",
            "yes",
            "--appendfsync",
            "always",
        ])
        .start()
        .await
        .expect("iniciar Valkey");
    TestValkey {
        container,
        url: format!("valkey://127.0.0.1:{port}"),
    }
}

async fn client(url: &str) -> Client {
    valkey_client::connect_with_retry(&ValkeyConfig::plain(url))
        .await
        .unwrap()
}

fn tuning() -> Tuning {
    Tuning {
        batch_size: 50,
        block: Duration::from_millis(200),
        claim_idle: Duration::from_millis(1_500),
        max_deliveries: 3,
        checkpoint_every: Duration::from_secs(1),
    }
}

fn event_json(session: Uuid, n: usize) -> String {
    let event = RecordEvent::new(EventKind::CitizenMessage, "bff/test")
        .session(session)
        .attr("n", n);
    let record = StreamRecord {
        event,
        sealed: Some(SealedPayload {
            alg: "A256GCM".into(),
            key_ref: KeyRef::Session {
                session_id: session,
            },
            nonce: B64.encode([n as u8; 12]),
            ciphertext: B64.encode(format!("cifrado-{n}")),
        }),
    };
    serde_json::to_string(&record).unwrap()
}

/// Publica `n` eventos repartidos en `sessions` sesiones. Devuelve sus event_id.
async fn publish(client: &Client, n: usize, sessions: usize) -> Vec<Uuid> {
    let ids: Vec<Uuid> = (0..sessions).map(|_| Uuid::now_v7()).collect();
    let mut event_ids = Vec::new();
    for i in 0..n {
        let json = event_json(ids[i % sessions], i);
        let record: StreamRecord = serde_json::from_str(&json).unwrap();
        event_ids.push(record.event.event_id);
        let _: String = client
            .xadd(RECORD_STREAM, false, None, "*", vec![("e", json)])
            .await
            .unwrap();
    }
    event_ids
}

struct Running {
    stop: watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Running {
    async fn start(valkey_url: &str, db: &TestDb, name: &str) -> Self {
        let consumer = Arc::new(Consumer {
            client: client(valkey_url).await,
            writer: Writer::new(db.pool("sb_writer")),
            consumer: name.into(),
            tuning: tuning(),
            stats: Default::default(),
        });
        let (stop, rx) = watch::channel(false);
        let tasks = record_writer::spawn(&consumer, &rx);
        Self { stop, tasks }
    }

    async fn shutdown(self) {
        let _ = self.stop.send(true);
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

async fn stored(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM registry.records")
        .fetch_one(&db.superuser)
        .await
        .unwrap_or(-1)
}

async fn wait_stored(db: &TestDb, expected: i64, within: Duration) -> i64 {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        let count = stored(db).await;
        if count >= expected || tokio::time::Instant::now() > deadline {
            return count;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn assert_exactly_once(db: &TestDb, expected_ids: &[Uuid]) {
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT event_id FROM registry.records ORDER BY record_id")
            .fetch_all(&db.superuser)
            .await
            .unwrap();
    assert_eq!(
        ids.len(),
        expected_ids.len(),
        "cantidad de registros distinta a la publicada"
    );
    let mut stored: Vec<_> = ids.clone();
    stored.sort();
    stored.dedup();
    assert_eq!(stored.len(), ids.len(), "hay registros duplicados");
    for id in expected_ids {
        assert!(ids.contains(id), "falta el evento {id}");
    }
    let report = verify::verify(&db.pool("sb_auditor"), None).await.unwrap();
    assert!(report.is_ok(), "la cadena no verifica: {:?}", report.issues);
}

async fn pending(client: &Client) -> u64 {
    let summary: (u64, Option<String>, Option<String>, fred::types::Value) = client
        .xpending(RECORD_STREAM, RECORD_GROUP, ())
        .await
        .unwrap();
    summary.0
}

async fn dead_letters(client: &Client) -> u64 {
    client.xlen(DEAD_LETTER_STREAM).await.unwrap()
}

#[tokio::test]
async fn every_event_is_stored_exactly_once_with_two_writers() {
    let valkey = start_valkey().await;
    let db = TestDb::start(None).await;
    let c = client(&valkey.url).await;
    // Eventos publicados ANTES de que exista record-writer también se guardan.
    let mut ids = publish(&c, 150, 12).await;
    let w1 = Running::start(&valkey.url, &db, "rw-1").await;
    let w2 = Running::start(&valkey.url, &db, "rw-2").await;
    ids.extend(publish(&c, 350, 12).await);

    assert_eq!(wait_stored(&db, 500, Duration::from_secs(30)).await, 500);
    assert_exactly_once(&db, &ids).await;
    assert_eq!(pending(&c).await, 0);
    assert_eq!(dead_letters(&c).await, 0);
    w1.shutdown().await;
    w2.shutdown().await;
}

#[tokio::test]
async fn postgres_down_events_wait_in_the_stream_and_nothing_goes_to_dead_letter() {
    let valkey = start_valkey().await;
    let db = TestDb::start(Some(free_port())).await;
    let c = client(&valkey.url).await;
    let writer = Running::start(&valkey.url, &db, "rw-1").await;

    // Postgres se cae.
    db.container.stop().await.unwrap();
    let ids = publish(&c, 120, 5).await;
    // Más tiempo que claim_idle × max_deliveries: un writer vivo no debe
    // marcar sus eventos como fallidos aunque Postgres siga caído.
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(
        dead_letters(&c).await,
        0,
        "una caída de Postgres no debe mandar nada a la cola de errores"
    );

    // Postgres vuelve: todo llega, exactamente una vez.
    db.container.start().await.unwrap();
    assert_eq!(wait_stored(&db, 120, Duration::from_secs(60)).await, 120);
    assert_exactly_once(&db, &ids).await;
    assert_eq!(pending(&c).await, 0);
    writer.shutdown().await;
}

#[tokio::test]
async fn writer_killed_mid_batch_is_recovered_by_another_without_duplicates() {
    let valkey = start_valkey().await;
    let db = TestDb::start(None).await;
    let c = client(&valkey.url).await;
    let ids = publish(&c, 200, 8).await;

    // Un writer lee un lote y guarda PARTE en Postgres, pero muere antes del
    // XACK (el peor momento posible). Se simula paso a paso para controlar
    // exactamente dónde muere.
    let dead = Consumer {
        client: client(&valkey.url).await,
        writer: Writer::new(db.pool("sb_writer")),
        consumer: "rw-dead".into(),
        tuning: tuning(),
        stats: Default::default(),
    };
    dead.ensure_group().await.unwrap();
    let mut streams = c
        .xreadgroup_map::<String, String, String, String, _, _, _, _>(
            RECORD_GROUP,
            "rw-dead",
            Some(100),
            None,
            false,
            RECORD_STREAM,
            ">",
        )
        .await
        .unwrap();
    let entries: RawEntries = streams.remove(RECORD_STREAM).unwrap();
    let partial: Vec<_> = entries
        .iter()
        .take(60)
        .map(|(id, f)| registry::writer::Entry {
            stream_id: id.clone(),
            record: serde_json::from_str(&f["e"]).unwrap(),
        })
        .collect();
    Writer::new(db.pool("sb_writer"))
        .write_batch(&partial)
        .await
        .unwrap();
    assert_eq!(stored(&db).await, 60);

    // Otro writer reclama los pendientes abandonados y termina el trabajo.
    let rescuer = Running::start(&valkey.url, &db, "rw-2").await;
    assert_eq!(wait_stored(&db, 200, Duration::from_secs(30)).await, 200);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_exactly_once(&db, &ids).await;
    assert_eq!(pending(&c).await, 0);
    assert_eq!(dead_letters(&c).await, 0);
    rescuer.shutdown().await;
}

#[tokio::test]
async fn valkey_restart_keeps_unsaved_events() {
    let valkey = start_valkey().await;
    let db = TestDb::start(None).await;
    let c = client(&valkey.url).await;
    let ids = publish(&c, 100, 4).await;

    // Valkey se reinicia ANTES de que record-writer los guarde.
    valkey.container.stop().await.unwrap();
    valkey.container.start().await.unwrap();
    let c = client(&valkey.url).await;
    let len: u64 = c.xlen(RECORD_STREAM).await.unwrap();
    assert_eq!(
        len, 100,
        "AOF debe conservar todos los eventos tras el reinicio"
    );

    // Y otra vez a mitad del procesamiento.
    let writer = Running::start(&valkey.url, &db, "rw-1").await;
    let more = publish(&c, 100, 4).await;
    valkey.container.stop().await.unwrap();
    valkey.container.start().await.unwrap();
    assert_eq!(wait_stored(&db, 200, Duration::from_secs(60)).await, 200);
    let all: Vec<Uuid> = ids.into_iter().chain(more).collect();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_exactly_once(&db, &all).await;
    writer.shutdown().await;
}

#[tokio::test]
async fn poison_events_go_to_dead_letter_without_blocking_the_rest() {
    let valkey = start_valkey().await;
    let db = TestDb::start(None).await;
    let c = client(&valkey.url).await;
    let mut ids = publish(&c, 20, 2).await;
    // Entrada ilegible y entrada con contenido sensible SIN cifrar.
    let _: String = c
        .xadd(RECORD_STREAM, false, None, "*", vec![("e", "{no es json")])
        .await
        .unwrap();
    let mut plaintext =
        RecordEvent::new(EventKind::CitizenMessage, "bff/test").session(Uuid::now_v7());
    plaintext.sensitive = Some(events::SensitivePayload::Message {
        text: "en claro".into(),
    });
    let json = serde_json::to_string(&StreamRecord {
        event: plaintext,
        sealed: None,
    })
    .unwrap();
    let _: String = c
        .xadd(RECORD_STREAM, false, None, "*", vec![("e", json)])
        .await
        .unwrap();
    ids.extend(publish(&c, 20, 2).await);

    let writer = Running::start(&valkey.url, &db, "rw-1").await;
    assert_eq!(wait_stored(&db, 40, Duration::from_secs(30)).await, 40);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_exactly_once(&db, &ids).await;
    assert_eq!(dead_letters(&c).await, 2);
    assert_eq!(pending(&c).await, 0);
    // La cola de errores conserva la entrada original para reinyectarla.
    let dlq: RawEntries = c
        .xrange_values(DEAD_LETTER_STREAM, "-", "+", None)
        .await
        .unwrap();
    assert!(dlq.iter().all(|(_, f)| f.contains_key("e")
        && f.contains_key("reason")
        && f.contains_key("source_id")));
    writer.shutdown().await;
}

#[tokio::test]
async fn checkpoints_are_created_and_verify() {
    let valkey = start_valkey().await;
    let db = TestDb::start(None).await;
    let c = client(&valkey.url).await;
    let writer = Running::start(&valkey.url, &db, "rw-1").await;
    publish(&c, 30, 3).await;
    assert_eq!(wait_stored(&db, 30, Duration::from_secs(20)).await, 30);
    // Esperar a que un checkpoint cubra los 30 registros.
    let mut covered = 0_i64;
    for _ in 0..50 {
        covered = sqlx::query_scalar(
            "SELECT COALESCE(sum(records_count), 0)::bigint FROM registry.checkpoints",
        )
        .fetch_one(&db.superuser)
        .await
        .unwrap();
        if covered == 30 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(covered, 30);
    let report = verify::verify(&db.pool("sb_auditor"), None).await.unwrap();
    assert!(report.is_ok(), "{:?}", report.issues);
    assert!(report.checkpoints >= 1);
    assert_eq!(report.records_after_last_checkpoint, 0);
    writer.shutdown().await;
}
