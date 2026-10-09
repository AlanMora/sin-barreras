-- Marca un comando como reenviado al C5, solo si la réplica sigue siendo dueña.
-- KEYS: 1 fwd, 2 sess. ARGV: 1 owner, 2 stream_id, 3.. pares campo/valor
if redis.call('GET', KEYS[1]) ~= ARGV[1] then
  return 0
end
redis.call('HSET', KEYS[2], 'fwd_id', ARGV[2])
for i = 3, #ARGV, 2 do
  redis.call('HSET', KEYS[2], ARGV[i], ARGV[i + 1])
end
return 1
