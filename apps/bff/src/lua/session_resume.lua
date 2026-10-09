-- KEYS: 1 sess, 2 seq, 3 out, 4 in, 5 cmids, 6 sb:rec:events
-- ARGV: 1 presented_token_hash, 2 new_token_hash, 3 registros (si se reanuda)
-- Devuelve {estado, seq_actual}. El token se rota en cada reanudación.
-- La vigencia del token es la de la sesión (TTL deslizante, ver ADR-0010).
local h = redis.call('HMGET', KEYS[1], 'token_hash', 'closed')
if not h[1] then
  return {'missing', '0'}
end
if h[2] == '1' then
  return {'closed', '0'}
end
if h[1] ~= ARGV[1] then
  return {'bad_token', '0'}
end
redis.call('HSET', KEYS[1], 'token_hash', ARGV[2])
sb_record(KEYS[6], ARGV[3], nil)
sb_touch(KEYS[1], {KEYS[1], KEYS[2], KEYS[3], KEYS[4], KEYS[5]})
return {'ok', redis.call('GET', KEYS[2]) or '0'}
