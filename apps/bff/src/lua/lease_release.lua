-- Libera el lease solo si no quedan comandos pendientes.
-- KEYS: 1 fwd, 2 sess, 3 in, 4 sb:sweep. ARGV: 1 owner, 2 sid
-- Devuelve 'released' | 'pending' | 'not_owner'
if redis.call('GET', KEYS[1]) ~= ARGV[1] then
  return 'not_owner'
end
local last = redis.call('XREVRANGE', KEYS[3], '+', '-', 'COUNT', 1)
local forwarded = redis.call('HGET', KEYS[2], 'fwd_id') or '0-0'
if #last > 0 and last[1][1] ~= forwarded then
  return 'pending'
end
redis.call('DEL', KEYS[1])
redis.call('ZREM', KEYS[4], ARGV[2])
return 'released'
