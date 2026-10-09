-- Renueva el TTL deslizante de la sesión.
-- KEYS: llaves de la sesión (la primera es sess). NUNCA incluir sb:rec:events.
if redis.call('EXISTS', KEYS[1]) == 0 then
  return 0
end
sb_touch(KEYS[1], KEYS)
return 1
