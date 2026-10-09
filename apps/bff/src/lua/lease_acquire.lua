-- Adquiere o renueva el lease de reenvío al C5 de una sesión.
-- KEYS: 1 fwd. ARGV: 1 owner, 2 ttl_ms. Devuelve 1 si la réplica es dueña.
local current = redis.call('GET', KEYS[1])
if not current then
  redis.call('SET', KEYS[1], ARGV[1], 'PX', ARGV[2])
  return 1
end
if current == ARGV[1] then
  redis.call('PEXPIRE', KEYS[1], ARGV[2])
  return 1
end
return 0
