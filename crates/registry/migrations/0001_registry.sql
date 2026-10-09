-- Registro completo del lado ciudadano de Sin Barreras.
--
-- Se ejecuta como `sb_owner` (dueño del esquema, sin login). Los roles se
-- crean en la inicialización del clúster (infra/postgres/initdb).
--
-- Garantías aplicadas por la BASE DE DATOS (no solo por el código):
--   * Solo inserción: ningún rol de aplicación tiene UPDATE/DELETE/TRUNCATE y,
--     además, triggers rechazan cualquier modificación o borrado.
--   * Mínimo privilegio por columna: record-writer solo puede LEER las columnas
--     que necesita para encadenar (no el contenido).
--   * Reportes solo ven vistas sin contenido sensible.

CREATE SCHEMA registry;
CREATE SCHEMA reporting;
REVOKE ALL ON SCHEMA registry, reporting FROM PUBLIC;

-- ---------------------------------------------------------------------------
-- Registros
-- ---------------------------------------------------------------------------
CREATE TABLE registry.records (
    record_id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event_id              uuid        NOT NULL UNIQUE,
    schema_version        smallint    NOT NULL,
    -- Cadena de integridad: una por sesión (o `system-AAAA-MM-DD`).
    chain_id              text        NOT NULL,
    chain_seq             bigint      NOT NULL CHECK (chain_seq > 0),
    kind                  text        NOT NULL,
    occurred_at           timestamptz NOT NULL,  -- hora del servidor (BFF)
    recorded_at           timestamptz NOT NULL,  -- hora de escritura (record-writer)
    client_ts             timestamptz,           -- hora del dispositivo
    session_id            uuid,
    incident_id           uuid,
    c5_incident_id        text,
    producer              text        NOT NULL,
    trace_id              text,
    stream_id             text        NOT NULL,  -- id de la entrada en Valkey
    -- Atributos NO sensibles, en JSON canónico exactamente como se hashearon.
    attributes_json       text        NOT NULL,
    attributes            jsonb       GENERATED ALWAYS AS (attributes_json::jsonb) STORED,
    -- Contenido sensible cifrado (AES-256-GCM). NULL si el evento no tiene.
    sealed_alg            text,
    sealed_key_ref        text,                  -- JSON de KeyRef
    sealed_nonce          bytea,
    sealed_ciphertext     bytea,
    prev_hash             bytea       NOT NULL CHECK (octet_length(prev_hash) = 32),
    hash                  bytea       NOT NULL CHECK (octet_length(hash) = 32),
    UNIQUE (chain_id, chain_seq),
    CHECK ((sealed_ciphertext IS NULL) = (sealed_nonce IS NULL)),
    CHECK ((sealed_ciphertext IS NULL) = (sealed_alg IS NULL))
);

CREATE INDEX records_session_idx  ON registry.records (session_id, occurred_at);
CREATE INDEX records_incident_idx ON registry.records (incident_id);
CREATE INDEX records_c5_idx       ON registry.records (c5_incident_id);
CREATE INDEX records_kind_idx     ON registry.records (kind, occurred_at);

-- DEK de cada sesión envuelta para la llave de investigación (HPKE).
-- Borrar una fila aquí = borrado criptográfico de la sesión. Solo se hará con
-- un procedimiento aprobado por el área jurídica (ver ADR-0016).
CREATE TABLE registry.session_keys (
    session_id        uuid        NOT NULL,
    recipient_key_id  text        NOT NULL,
    alg               text        NOT NULL,
    encapped          bytea       NOT NULL,
    wrapped_key       bytea       NOT NULL,
    created_at        timestamptz NOT NULL,
    source_event_id   uuid        NOT NULL,
    PRIMARY KEY (session_id, recipient_key_id)
);

-- Checkpoints: encadenan TODOS los registros por rangos de record_id. Detectan
-- el borrado de cadenas completas (que la cadena por sesión no ve).
CREATE TABLE registry.checkpoints (
    checkpoint_id     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    created_at        timestamptz NOT NULL,
    covered_from      bigint      NOT NULL,   -- record_id inicial (exclusivo)
    covered_until     bigint      NOT NULL,   -- record_id final (inclusivo)
    records_count     bigint      NOT NULL,
    batch_root        bytea       NOT NULL CHECK (octet_length(batch_root) = 32),
    prev_hash         bytea       NOT NULL CHECK (octet_length(prev_hash) = 32),
    hash              bytea       NOT NULL CHECK (octet_length(hash) = 32),
    CHECK (covered_until >= covered_from)
);

-- ---------------------------------------------------------------------------
-- Solo inserción
-- ---------------------------------------------------------------------------
CREATE FUNCTION registry.forbid_modification() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'registro de solo inserción: % prohibido en %.%', TG_OP, TG_TABLE_SCHEMA, TG_TABLE_NAME
        USING ERRCODE = 'insufficient_privilege';
END
$$;

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['records', 'session_keys', 'checkpoints'] LOOP
        EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON registry.%I
                        FOR EACH ROW EXECUTE FUNCTION registry.forbid_modification()', t || '_append_only', t);
        EXECUTE format('CREATE TRIGGER %I BEFORE TRUNCATE ON registry.%I
                        FOR EACH STATEMENT EXECUTE FUNCTION registry.forbid_modification()', t || '_no_truncate', t);
    END LOOP;
END
$$;

-- ---------------------------------------------------------------------------
-- Permisos (mínimo privilegio)
-- ---------------------------------------------------------------------------
-- record-writer: insertar y leer solo lo necesario para encadenar.
GRANT USAGE ON SCHEMA registry TO sb_writer;
GRANT INSERT ON registry.records, registry.session_keys, registry.checkpoints TO sb_writer;
GRANT SELECT (record_id, event_id, chain_id, chain_seq, hash) ON registry.records TO sb_writer;
GRANT SELECT (session_id, recipient_key_id) ON registry.session_keys TO sb_writer;
GRANT SELECT ON registry.checkpoints TO sb_writer;

-- Auditoría de integridad: leer todo lo hasheado (el contenido sigue cifrado).
GRANT USAGE ON SCHEMA registry TO sb_auditor;
GRANT SELECT ON registry.records, registry.checkpoints TO sb_auditor;

-- Reportes: solo el esquema `reporting` (vistas sin contenido sensible).
GRANT USAGE ON SCHEMA reporting TO sb_reporting;
