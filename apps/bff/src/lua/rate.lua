-- Contador de ventana fija. KEYS: 1 contador. ARGV: 1 ventana en ms.
local n = redis.call('INCR', KEYS[1])
if n == 1 then
  redis.call('PEXPIRE', KEYS[1], ARGV[1])
end
return n
