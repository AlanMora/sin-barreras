-- Acepta un comando del ciudadano de forma durable e idempotente, junto con
-- su registro.
-- KEYS: 1 sess, 2 seq, 3 out, 4 in, 5 cmids, 6 fwd, 7 sb:sweep, 8 sb:rec:events
-- ARGV: 1 sid, 2 client_msg_id, 3 msg_json, 4 now_ms, 5 forward ('1'|'0'),
--       6 echo_payload ('' si no hay), 7 registros
-- Devuelve {'missing'} | {'duplicate', ref} | {'accepted', ref, echo_seq}
if redis.call('EXISTS', KEYS[1]) == 0 then
  return {'missing'}
end
local prev = redis.call('HGET', KEYS[5], ARGV[2])
if prev then
  return {'duplicate', prev}
end
local ref = 'local'
if ARGV[5] == '1' then
  ref = redis.call('XADD', KEYS[4], '*', 'm', ARGV[3], 'ts', ARGV[4], 'cmid', ARGV[2])
  redis.call('ZADD', KEYS[7], 'NX', ARGV[4], ARGV[1])
end
redis.call('HSET', KEYS[5], ARGV[2], ref)
local seq = 0
if ARGV[6] ~= '' then
  seq = sb_emit(KEYS[2], KEYS[3], ARGV[1], ARGV[6])
end
sb_record(KEYS[8], ARGV[7], seq)
sb_touch(KEYS[1], {KEYS[1], KEYS[2], KEYS[3], KEYS[4], KEYS[5]})
return {'accepted', ref, string.format('%d', seq)}
