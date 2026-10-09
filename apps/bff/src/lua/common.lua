-- Funciones comunes que se anteponen a cada script del BFF.
--
-- Llaves de una sesión (todas con hash tag {sid} para quedar en el mismo slot):
--   sb:s:{sid}        HASH    estado de la sesión
--   sb:s:{sid}:seq    STRING  contador de seq
--   sb:s:{sid}:out    STREAM  mensajes secuenciados al ciudadano (ID = <seq>-0)
--   sb:s:{sid}:in     STREAM  comandos del ciudadano pendientes de enviar al C5
--   sb:s:{sid}:cmids  HASH    client_msg_id ya recibidos (idempotencia)
--   sb:s:{sid}:fwd    STRING  lease del reenvío al C5 (réplica dueña)

--
-- Registro completo (global, SIN TTL — nunca debe pasarse a sb_touch):
--   sb:rec:events     STREAM  eventos pendientes de guardar en Postgres
--
-- Renueva el TTL de todas las llaves de la sesión con el TTL guardado en
-- la propia sesión (cambia cuando se abre o cierra un incidente).
local function sb_touch(sess_key, keys)
  local ttl = tonumber(redis.call('HGET', sess_key, 'ttl_ms') or '0')
  if ttl and ttl > 0 then
    for _, k in ipairs(keys) do
      redis.call('PEXPIRE', k, ttl)
    end
  end
end

-- Asigna el siguiente seq, guarda el frame en el stream de salida y lo
-- publica en el bus. Todo atómico: nunca hay seq sin frame ni frame sin seq.
-- `payload` es el JSON de un ServerEvent (objeto que empieza con '{').
local function sb_emit(seq_key, out_key, sid, payload)
  local seq = redis.call('INCR', seq_key)
  local frame = '{"seq":' .. string.format('%d', seq) .. ',' .. string.sub(payload, 2)
  redis.call('XADD', out_key, string.format('%d', seq) .. '-0', 'f', frame)
  redis.call('PUBLISH', 'sb:bus', sid .. '|' .. string.format('%d', seq) .. '|' .. frame)
  return seq
end

-- Agrega al stream del registro las entradas de `blob` (JSON separados por
-- salto de línea; el JSON compacto nunca contiene saltos de línea sin
-- escapar). Si se indica `seq`, sustituye el marcador "__SEQ__" por el seq
-- asignado, para que el registro diga exactamente qué seq vio el ciudadano.
local function sb_record(rec_key, blob, seq)
  if not blob or blob == '' then
    return
  end
  for entry in string.gmatch(blob, '[^\n]+') do
    if seq then
      entry = string.gsub(entry, '"__SEQ__"', string.format('%d', seq))
    end
    redis.call('XADD', rec_key, '*', 'e', entry)
  end
end
