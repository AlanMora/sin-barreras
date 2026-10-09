-- Emite un mensaje secuenciado a la sesión y, opcionalmente, actualiza campos.
-- KEYS: 1 sess, 2 seq, 3 out, 4 in, 5 cmids, 6 dedupe ('sb:nodedupe' si no aplica)
-- ARGV: 1 sid, 2 payload, 3 dedupe_ttl_ms, 4.. pares campo/valor para la sesión
-- Devuelve: seq (>0), 0 si es duplicado, -1 si la sesión ya no existe.
if redis.call('EXISTS', KEYS[1]) == 0 then
  return -1
end
if KEYS[6] ~= 'sb:nodedupe' then
  local fresh = redis.call('SET', KEYS[6], '1', 'NX', 'PX', ARGV[3])
  if not fresh then
    return 0
  end
end
for i = 4, #ARGV, 2 do
  redis.call('HSET', KEYS[1], ARGV[i], ARGV[i + 1])
end
local seq = sb_emit(KEYS[2], KEYS[3], ARGV[1], ARGV[2])
sb_touch(KEYS[1], {KEYS[1], KEYS[2], KEYS[3], KEYS[4], KEYS[5]})
return seq
