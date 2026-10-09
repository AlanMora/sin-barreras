-- Investigación con bitácora de accesos OBLIGATORIA.
--
-- sb_investigator no tiene SELECT sobre ninguna tabla: solo puede ejecutar
-- estas funciones, que PRIMERO registran el acceso (quién, cuándo, qué, por
-- qué) en una bitácora de solo inserción y DESPUÉS devuelven los datos. No hay
-- forma de leer el registro sensible sin dejar rastro.
--
-- Se ejecutan en el PRIMARIO (escriben la bitácora); la réplica es de solo
-- lectura.

CREATE SCHEMA investigation;
REVOKE ALL ON SCHEMA investigation FROM PUBLIC;

CREATE TABLE registry.access_log (
    access_id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    accessed_at    timestamptz NOT NULL DEFAULT clock_timestamp(),
    db_user        text        NOT NULL DEFAULT session_user,
    client_addr    inet                 DEFAULT inet_client_addr(),
    requested_by   text        NOT NULL CHECK (length(trim(requested_by)) >= 3),
    reason         text        NOT NULL CHECK (length(trim(reason)) >= 5),
    action         text        NOT NULL,
    session_id     uuid,
    c5_incident_id text,
    rows_returned  integer
);

CREATE TRIGGER access_log_append_only BEFORE UPDATE OR DELETE ON registry.access_log
    FOR EACH ROW EXECUTE FUNCTION registry.forbid_modification();
CREATE TRIGGER access_log_no_truncate BEFORE TRUNCATE ON registry.access_log
    FOR EACH STATEMENT EXECUTE FUNCTION registry.forbid_modification();

-- Buscar sesiones por id del C5 (para cruzar con una queja o investigación).
CREATE FUNCTION investigation.find_sessions(p_c5_incident_id text, p_requested_by text, p_reason text)
RETURNS TABLE (session_id uuid, incident_id uuid, first_event timestamptz, last_event timestamptz)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, registry AS $$
DECLARE n integer;
BEGIN
    RETURN QUERY
        SELECT r.session_id, max(r.incident_id::text)::uuid, min(r.occurred_at), max(r.occurred_at)
        FROM registry.records r
        WHERE r.session_id IN (SELECT DISTINCT x.session_id FROM registry.records x WHERE x.c5_incident_id = p_c5_incident_id)
        GROUP BY r.session_id;
    GET DIAGNOSTICS n = ROW_COUNT;
    INSERT INTO registry.access_log (requested_by, reason, action, c5_incident_id, rows_returned)
    VALUES (p_requested_by, p_reason, 'find_sessions', p_c5_incident_id, n);
END
$$;

-- Todos los registros de una sesión (contenido aún cifrado).
CREATE FUNCTION investigation.read_session(p_session_id uuid, p_requested_by text, p_reason text)
RETURNS SETOF registry.records
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, registry AS $$
DECLARE n integer;
BEGIN
    -- Primero la bitácora: si no se puede registrar el acceso, no hay datos.
    INSERT INTO registry.access_log (requested_by, reason, action, session_id)
    VALUES (p_requested_by, p_reason, 'read_session', p_session_id);
    RETURN QUERY SELECT * FROM registry.records WHERE session_id = p_session_id ORDER BY chain_seq;
    GET DIAGNOSTICS n = ROW_COUNT;
    INSERT INTO registry.access_log (requested_by, reason, action, session_id, rows_returned)
    VALUES (p_requested_by, p_reason, 'read_session_rows', p_session_id, n);
END
$$;

-- Llave envuelta de una sesión (necesaria para descifrar con la llave privada).
CREATE FUNCTION investigation.session_keys(p_session_id uuid, p_requested_by text, p_reason text)
RETURNS SETOF registry.session_keys
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, registry AS $$
BEGIN
    INSERT INTO registry.access_log (requested_by, reason, action, session_id)
    VALUES (p_requested_by, p_reason, 'session_keys', p_session_id);
    RETURN QUERY SELECT * FROM registry.session_keys WHERE session_id = p_session_id;
END
$$;

REVOKE ALL ON ALL FUNCTIONS IN SCHEMA investigation FROM PUBLIC;
GRANT USAGE ON SCHEMA investigation TO sb_investigator;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA investigation TO sb_investigator;

-- La auditoría de integridad también puede revisar la bitácora.
GRANT SELECT ON registry.access_log TO sb_auditor;
