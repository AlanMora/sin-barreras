//! Postgres 18 real en contenedor, con roles y migraciones aplicados, para
//! pruebas de integración (feature `test-support`).

#![allow(clippy::expect_used)]

use sqlx::PgPool;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, ImageExt, core::ContainerPort, runners::AsyncRunner},
};

use crate::db::{DbConfig, MIGRATOR, ROLES_SQL, migrator_config};

pub struct TestDb {
    pub container: ContainerAsync<Postgres>,
    pub port: u16,
    pub superuser: PgPool,
}

impl TestDb {
    /// Arranca Postgres. Con `fixed_port`, el puerto del host sobrevive a
    /// `stop()`/`start()` (para simular caídas reales).
    pub async fn start(fixed_port: Option<u16>) -> Self {
        let mut request = Postgres::default().with_tag("18-alpine");
        if let Some(port) = fixed_port {
            request = request.with_mapped_port(port, ContainerPort::Tcp(5432));
        }
        let container = request.start().await.expect("iniciar Postgres");
        let port = container.get_host_port_ipv4(5432).await.expect("puerto");
        let db = Self {
            container,
            port,
            superuser: PgPool::connect_lazy("postgres://x@localhost/x").expect("pool"),
        };
        let superuser = db.config("postgres").pool(2);
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
            .execute(&superuser)
            .await
            .expect("roles");
        MIGRATOR
            .run(&migrator_config(db.config("sb_migrator")).pool(1))
            .await
            .expect("migraciones");
        Self { superuser, ..db }
    }

    pub fn config(&self, user: &str) -> DbConfig {
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

    pub fn pool(&self, user: &str) -> PgPool {
        self.config(user).pool(4)
    }
}

/// Puerto TCP libre en este momento.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("puerto libre")
}
