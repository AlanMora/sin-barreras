//! Conexión a Postgres con TLS verificado y migraciones.

use std::{path::PathBuf, time::Duration};

use service_kit::config::{ConfigError, env_or, env_required, secret};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    /// CA con la que se verifica el certificado del servidor. Si es `None`
    /// no se usa TLS (solo pruebas con contenedores efímeros).
    pub ca_file: Option<PathBuf>,
    /// Rol a asumir al conectar (`SET ROLE`), p. ej. `sb_owner` para migrar.
    pub role: Option<String>,
    /// `search_path` de la sesión (migraciones: `sb_migrations`).
    pub search_path: Option<String>,
    pub application_name: String,
}

impl DbConfig {
    /// Lee `PG_HOST`, `PG_PORT`, `PG_DATABASE`, `PG_USER`, `PG_PASSWORD(_FILE)`
    /// y `PG_CA_FILE` (si existe, TLS con verificación completa).
    pub fn from_env(application_name: &str) -> Result<Self, ConfigError> {
        Ok(Self {
            host: env_or("PG_HOST", "postgres".to_owned())?,
            port: env_or("PG_PORT", 5432)?,
            database: env_or("PG_DATABASE", "sin_barreras".to_owned())?,
            user: env_required("PG_USER")?,
            password: secret("PG_PASSWORD")?,
            ca_file: std::env::var("PG_CA_FILE").ok().map(PathBuf::from),
            role: None,
            search_path: None,
            application_name: application_name.to_owned(),
        })
    }

    pub fn options(&self) -> PgConnectOptions {
        let mut options = PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .database(&self.database)
            .username(&self.user)
            .password(&self.password)
            .application_name(&self.application_name);
        options = match &self.ca_file {
            Some(ca) => options.ssl_mode(PgSslMode::VerifyFull).ssl_root_cert(ca),
            None => options.ssl_mode(PgSslMode::Disable),
        };
        if let Some(role) = &self.role {
            options = options.options([("role", role.as_str())]);
        }
        if let Some(path) = &self.search_path {
            options = options.options([("search_path", path.as_str())]);
        }
        options
    }

    /// Pool perezoso: Postgres caído no impide que el proceso arranque; conecta
    /// bajo demanda y, mientras tanto, los eventos esperan en el stream.
    pub fn pool(&self, max_connections: u32) -> PgPool {
        PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect_lazy_with(self.options())
    }
}

/// Migraciones versionadas del registro (embebidas en el binario).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// SQL de creación de roles (con variables de psql). Fuente única para la
/// inicialización del clúster y para las pruebas.
pub const ROLES_SQL: &str = include_str!("../sql/roles.sql");

/// Configuración de conexión para migrar: como `sb_migrator`, asumiendo
/// `sb_owner`, con la tabla de control en `sb_migrations`.
pub fn migrator_config(mut base: DbConfig) -> DbConfig {
    base.role = Some("sb_owner".into());
    base.search_path = Some("sb_migrations".into());
    base
}
