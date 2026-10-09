-- Roles de Sin Barreras (mínimo privilegio). Se ejecuta UNA vez, como
-- superusuario, al inicializar el clúster (infra/postgres/initdb/10-roles.sh).
-- Las contraseñas llegan como variables de psql desde Docker secrets.
--
--   sb_owner        dueño de los esquemas. SIN login.
--   sb_migrator     aplica migraciones asumiendo sb_owner (no lo hereda).
--   sb_writer       record-writer: INSERT y lectura mínima para encadenar.
--   sb_auditor      verificación de integridad: solo lectura.
--   sb_reporting    tableros: solo vistas sin contenido sensible (réplica).
--   sb_investigator consultas de investigación vía funciones auditadas (Fase 7).
--   sb_replicator   replicación de streaming hacia la réplica (Fase 8).

CREATE ROLE sb_owner NOLOGIN;
CREATE ROLE sb_migrator LOGIN PASSWORD :'migrator_password';
GRANT sb_owner TO sb_migrator WITH INHERIT FALSE, SET TRUE;
CREATE ROLE sb_writer LOGIN PASSWORD :'writer_password';
CREATE ROLE sb_auditor LOGIN PASSWORD :'auditor_password';
CREATE ROLE sb_reporting LOGIN PASSWORD :'reporting_password';
CREATE ROLE sb_investigator LOGIN PASSWORD :'investigator_password';
CREATE ROLE sb_replicator LOGIN REPLICATION PASSWORD :'replicator_password';

-- Nadie entra a la base salvo los roles del sistema.
REVOKE ALL ON DATABASE :"dbname" FROM PUBLIC;
GRANT CONNECT ON DATABASE :"dbname" TO sb_migrator, sb_writer, sb_auditor, sb_reporting, sb_investigator;
GRANT CREATE ON DATABASE :"dbname" TO sb_owner;
REVOKE ALL ON SCHEMA public FROM PUBLIC;

-- Esquema donde sqlx guarda el control de migraciones.
CREATE SCHEMA sb_migrations AUTHORIZATION sb_owner;

-- Límites de seguridad por rol.
ALTER ROLE sb_writer SET statement_timeout = '30s';
ALTER ROLE sb_reporting SET statement_timeout = '60s';
ALTER ROLE sb_reporting SET default_transaction_read_only = on;
ALTER ROLE sb_auditor SET default_transaction_read_only = on;
-- Defensa en profundidad: toda sentencia del rol de investigación queda en el
-- log del servidor, además de la bitácora en la base.
ALTER ROLE sb_investigator SET log_statement = 'all';
ALTER ROLE sb_investigator SET statement_timeout = '120s';
