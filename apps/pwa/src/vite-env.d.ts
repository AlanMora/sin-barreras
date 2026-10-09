interface ImportMetaEnv {
  readonly VITE_FALLBACK_PHONE?: string;
  /** URL del WebSocket del BFF (por defecto, el mismo origen + /ws). */
  readonly VITE_WS_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
