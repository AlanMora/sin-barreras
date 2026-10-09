// ARCHIVO GENERADO desde crates/protocol. NO EDITAR A MANO.
// Regenerar: UPDATE_TS_BINDINGS=1 cargo test -p protocol --test typescript

export const PROTOCOL_VERSION = 1;
export const MAX_CHAT_TEXT_CHARS = 2000;
export const MAX_CLIENT_FRAME_BYTES = 16384;
export const HEARTBEAT_INTERVAL_MS = 15000;

/** Cualquier frame que envía el servidor. */
export type ServerFrame = ServerControl | SequencedMessage;

export type ClientMessage = { "type": "hello" } & Hello | { "type": "create_incident" } & CreateIncident | { "type": "chat_message" } & ClientChatMessage | { "type": "location_update" } & LocationUpdate | { "type": "video_call_request" } & VideoCallRequest | { "type": "client_event" } & ClientEvent | { "type": "ack" } & ClientAck | { "type": "ping" } & Ping | { "type": "close" } & ClientClose;

export type Hello = { client_msg_id: string, client_ts?: Timestamp, traceparent?: string, 
/**
 * Versión del protocolo que habla el cliente.
 */
protocol_version: number, 
/**
 * Versión de la PWA (`0.1.0`).
 */
app_version: string, 
/**
 * Idioma de la interfaz (BCP 47, p. ej. `es-MX`).
 */
language: string, 
/**
 * Presente si el cliente quiere reanudar una sesión previa.
 */
resume?: Resume, 
/**
 * Punto de entrada por QR (`/?t=TOT-0142&k=1&s=...`).
 */
entry?: QrEntry, 
/**
 * Ajustes de accesibilidad activos (se registran como metadatos).
 */
accessibility?: AccessibilitySettings, };

export type Resume = { session_id: string, session_token: string, 
/**
 * Último `seq` recibido y procesado; el BFF reenvía todo lo posterior.
 */
last_seq: number, };

export type QrEntry = { 
/**
 * Identificador del punto (`TOT-0142`).
 */
point_id: string, 
/**
 * Identificador de la llave con la que se firmó el QR.
 */
key_id?: string, 
/**
 * Firma HMAC del QR, base64url.
 */
signature: string, };

export type AccessibilitySettings = { 
/**
 * Escala de letra (1.0 = normal).
 */
font_scale: number, high_contrast: boolean, reduced_motion: boolean, silent_mode: boolean, };

export type Channel = "lsm_video" | "text_chat" | "audio" | "translation" | "silent";

export type CreateIncident = { client_msg_id: string, client_ts?: Timestamp, traceparent?: string, channel: Channel, 
/**
 * Idioma en que el ciudadano se comunicará (BCP 47). Relevante para
 * el canal `translation`.
 */
language?: string, };

export type ChatKind = "text" | "quick_reply";

export type QuickReply = "in_danger" | "cannot_speak" | "being_followed" | "injured";

export type ClientChatMessage = { client_msg_id: string, client_ts?: Timestamp, traceparent?: string, kind: ChatKind, 
/**
 * Texto tal como lo ve el ciudadano (en respuestas rápidas, el texto
 * localizado del botón).
 */
text: string, 
/**
 * Código de la respuesta rápida; obligatorio si `kind = quick_reply`.
 */
quick_reply?: QuickReply, };

export type LocationSource = "gps" | "network";

export type LocationUpdate = { client_msg_id: string, client_ts?: Timestamp, traceparent?: string, latitude: number, longitude: number, 
/**
 * Precisión reportada por el dispositivo, en metros.
 */
accuracy_m?: number, source: LocationSource, 
/**
 * Momento en que el dispositivo obtuvo la posición.
 */
captured_at: Timestamp, };

export type VideoCallRequest = { client_msg_id: string, client_ts?: Timestamp, traceparent?: string, };

export type ClientEventKind = "app_opened" | "privacy_notice_shown" | "privacy_notice_opened" | "channel_screen_shown" | "abandoned_before_help" | "quick_exit_used" | "settings_changed" | "permission_granted" | "permission_denied" | "went_offline" | "came_back_online" | "client_error";

export type ClientEvent = { client_msg_id: string, client_ts?: Timestamp, traceparent?: string, kind: ClientEventKind, 
/**
 * Detalle breve y no sensible (p. ej. `geolocation`, `font_scale=1.3`).
 */
detail?: string, };

export type ClientAck = { client_msg_id: string, client_ts?: Timestamp, seq: number, };

export type Ping = { client_msg_id: string, client_ts?: Timestamp, };

export type ClientCloseReason = "ended_by_citizen" | "abandoned_before_help";

export type ClientClose = { client_msg_id: string, client_ts?: Timestamp, reason: ClientCloseReason, };

export type ServerControl = { "type": "welcome" } & Welcome | { "type": "ack" } & ServerAck | { "type": "pong" } & Pong | { "type": "error" } & ServerError;

export type EntryVerification = "none" | "verified" | "unverified";

export type Welcome = { protocol_version: number, session_id: string, 
/**
 * Token anónimo para reanudar. Cambia en cada `welcome`: el anterior
 * queda invalidado.
 */
session_token: string, token_expires_at: Timestamp, 
/**
 * `true` si se reanudó la sesión pedida en `hello.resume`.
 */
resumed: boolean, entry: EntryVerification, heartbeat_interval_ms: number, 
/**
 * Último `seq` emitido en la sesión. Si es mayor que el `last_seq` del
 * cliente, a continuación llegan los mensajes faltantes.
 */
latest_seq: number, incident?: IncidentSummary, server_ts: Timestamp, };

export type IncidentSummary = { incident_id: string, channel: Channel, status: IncidentState, };

export type AckStatus = "accepted" | "duplicate";

export type ServerAck = { client_msg_id: string, status: AckStatus, server_ts: Timestamp, };

export type Pong = { client_msg_id: string, server_ts: Timestamp, };

export type ErrorCode = "unsupported_protocol_version" | "hello_required" | "invalid_message" | "message_too_large" | "rate_limited" | "internal";

export type ServerError = { code: ErrorCode, 
/**
 * Descripción técnica breve (no se muestra tal cual al ciudadano; la PWA
 * traduce el `code`).
 */
message: string, client_msg_id?: string, 
/**
 * Si el cliente puede reintentar el mismo mensaje.
 */
retryable: boolean, supported_versions?: Array<number>, server_ts: Timestamp, };

export type SequencedMessage = { seq: number, } & ({ "type": "incident_created" } & IncidentCreated | { "type": "chat_message" } & ServerChatMessage | { "type": "message_status" } & MessageStatus | { "type": "incident_status" } & IncidentStatus | { "type": "video_call" } & VideoCallUpdate | { "type": "session_closed" } & SessionClosed);

export type ServerEvent = { "type": "incident_created" } & IncidentCreated | { "type": "chat_message" } & ServerChatMessage | { "type": "message_status" } & MessageStatus | { "type": "incident_status" } & IncidentStatus | { "type": "video_call" } & VideoCallUpdate | { "type": "session_closed" } & SessionClosed;

export type IncidentCreated = { 
/**
 * Nuestro identificador del incidente (no el del C5).
 */
incident_id: string, channel: Channel, 
/**
 * `client_msg_id` del `create_incident` (o del mensaje que lo provocó).
 */
client_msg_id: string, server_ts: Timestamp, };

export type Author = "citizen" | "operator" | "system";

export type ServerChatMessage = { message_id: string, author: Author, kind: ChatKind, text: string, quick_reply?: QuickReply, client_msg_id?: string, server_ts: Timestamp, };

export type DeliveryStatus = "delivered" | "read" | "failed";

export type MessageStatus = { client_msg_id: string, status: DeliveryStatus, server_ts: Timestamp, };

export type IncidentState = "received" | "attending" | "unit_en_route" | "unit_on_scene" | "closed";

export type IncidentStatus = { status: IncidentState, 
/**
 * Tiempo estimado de llegada, en segundos.
 */
eta_seconds?: number, server_ts: Timestamp, };

export type VideoCallState = "requested" | "ready" | "failed" | "ended";

export type VideoCallUpdate = { request_client_msg_id: string, status: VideoCallState, join?: Record<string, unknown>, server_ts: Timestamp, };

export type SessionClosedReason = "incident_closed" | "ended_by_citizen" | "expired";

export type SessionClosed = { reason: SessionClosedReason, server_ts: Timestamp, };

export type Timestamp = string;
