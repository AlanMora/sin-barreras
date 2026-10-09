-- Vistas para tableros y métricas del programa. Sin contenido sensible: ni
-- texto cifrado, ni llaves, ni hashes. Se consultan en la réplica.

CREATE VIEW reporting.events AS
SELECT record_id, event_id, kind, occurred_at, recorded_at, client_ts,
       session_id, incident_id, c5_incident_id, producer, trace_id, attributes,
       (sealed_ciphertext IS NOT NULL) AS has_sensitive_content
FROM registry.records;

CREATE VIEW reporting.sessions AS
SELECT session_id,
       min(occurred_at)                                                            AS started_at,
       max(occurred_at)                                                            AS last_event_at,
       max(attributes->>'source')       FILTER (WHERE kind = 'entry_point')        AS entry_source,
       max(attributes->>'point_id')     FILTER (WHERE kind = 'entry_point')        AS point_id,
       max(attributes->>'verification') FILTER (WHERE kind = 'entry_point')        AS entry_verification,
       max(attributes->>'language')     FILTER (WHERE kind = 'client_metadata')    AS language,
       max(attributes->>'app_version')  FILTER (WHERE kind = 'client_metadata')    AS app_version,
       max(attributes->>'channel')      FILTER (WHERE kind = 'incident_created')   AS channel,
       max(incident_id::text)::uuid                                                AS incident_id,
       max(c5_incident_id)                                                         AS c5_incident_id,
       min(occurred_at)                 FILTER (WHERE kind = 'incident_requested') AS help_requested_at,
       min(occurred_at)                 FILTER (WHERE kind = 'incident_created')   AS incident_created_at,
       min(occurred_at)                 FILTER (WHERE kind = 'first_operator_response') AS first_response_at,
       bool_or(kind = 'client_event' AND attributes->>'event' = 'abandoned_before_help') AS abandoned_before_help,
       bool_or(kind = 'client_event' AND attributes->>'event' = 'quick_exit_used')       AS used_quick_exit,
       count(*) FILTER (WHERE kind = 'citizen_message')                            AS citizen_messages,
       count(*) FILTER (WHERE kind = 'operator_message')                           AS operator_messages,
       count(*) FILTER (WHERE kind = 'connection_opened')                          AS connections,
       count(*) FILTER (WHERE kind = 'location_received')                          AS location_updates,
       bool_or(kind = 'video_call_requested')                                      AS requested_video,
       max(attributes->>'status') FILTER (WHERE kind = 'incident_status_changed')  AS last_known_status
FROM registry.records
WHERE session_id IS NOT NULL
GROUP BY session_id;

-- Métricas diarias (hora de Guadalajara).
CREATE VIEW reporting.daily_metrics AS
SELECT (started_at AT TIME ZONE 'America/Mexico_City')::date                      AS day,
       count(*)                                                                    AS sessions,
       count(*) FILTER (WHERE incident_created_at IS NOT NULL)                     AS incidents,
       count(*) FILTER (WHERE abandoned_before_help)                               AS abandoned_before_help,
       count(*) FILTER (WHERE entry_source = 'qr')                                 AS from_qr,
       count(*) FILTER (WHERE entry_verification = 'unverified')                   AS unverified_qr,
       count(*) FILTER (WHERE channel = 'lsm_video')                               AS channel_lsm_video,
       count(*) FILTER (WHERE channel = 'text_chat')                               AS channel_text_chat,
       count(*) FILTER (WHERE channel = 'audio')                                   AS channel_audio,
       count(*) FILTER (WHERE channel = 'translation')                             AS channel_translation,
       count(*) FILTER (WHERE channel = 'silent')                                  AS channel_silent,
       percentile_cont(0.5) WITHIN GROUP (
           ORDER BY extract(epoch FROM first_response_at - incident_created_at))   AS median_first_response_s,
       percentile_cont(0.9) WITHIN GROUP (
           ORDER BY extract(epoch FROM first_response_at - incident_created_at))   AS p90_first_response_s
FROM reporting.sessions
GROUP BY 1;

GRANT SELECT ON ALL TABLES IN SCHEMA reporting TO sb_reporting;
