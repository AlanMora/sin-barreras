-- KEYS: 1 sess, 2 seq, 3 out, 4 in, 5 cmids
-- ARGV: 1 sid, 2 token_hash, 3 now_ms, 4 ttl_ms, 5.. pares campo/valor
if redis.call('EXISTS', KEYS[1]) == 1 then
  return 0
end
redis.call('HSET', KEYS[1], 'token_hash', ARGV[2], 'created_at', ARGV[3],
  'ttl_ms', ARGV[4], 'fwd_id', '0-0')
for i = 5, #ARGV, 2 do
  redis.call('HSET', KEYS[1], ARGV[i], ARGV[i + 1])
end
redis.call('SET', KEYS[2], 0)
sb_touch(KEYS[1], {KEYS[1], KEYS[2]})
return 1
