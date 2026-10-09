#!/usr/bin/env bash
# Prueba de restauración de respaldos (pgBackRest).
#
# 1. Hace un respaldo completo cifrado del primario.
# 2. Lo restaura en un Postgres DESECHABLE (volumen nuevo, sin archivado: nunca
#    escribe en el repositorio de respaldos del primario).
# 3. Verifica con `sb-admin verify` que la cadena de integridad del registro
#    restaurado está intacta y que no falta ningún registro.
#
# Uso (con el sistema de desarrollo levantado):  infra/backup/restore-test.sh
# En producción se ejecuta igual, en un nodo aislado (ver docs/runbooks/restauracion.md).
set -euo pipefail
cd "$(dirname "$0")/../.."

NET="${SB_NETWORK:-sin-barreras_default}"
REPO_VOLUME="${SB_BACKUP_VOLUME:-sin-barreras_pgbackrest-repo}"
IMAGE="${SB_POSTGRES_IMAGE:-sin-barreras/postgres:dev}"
ADMIN_IMAGE="${SB_ADMIN_IMAGE:-sin-barreras/sb-admin:dev}"
SECRETS="$PWD/infra/dev/secrets"
CERTS="$PWD/infra/dev/certs"
NAME="postgres-restore"
VOLUME="sb-restore-test-$$"
cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  docker volume rm "$VOLUME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "▶ 1/4 Respaldo completo cifrado del primario"
docker compose exec -T postgres gosu postgres pgbackrest --stanza=sin-barreras --type=full backup >/dev/null
docker compose exec -T postgres gosu postgres pgbackrest --stanza=sin-barreras info | sed 's/^/    /'
SOURCE_COUNT=$(docker compose exec -T postgres psql -U postgres -d sin_barreras -tAc "SELECT count(*) FROM registry.records")
echo "    registros en el primario al momento del respaldo: $SOURCE_COUNT"

echo "▶ 2/4 Restauración en un volumen nuevo"
docker volume create "$VOLUME" >/dev/null
docker run --rm \
  -v "$VOLUME":/var/lib/postgresql \
  -v "$REPO_VOLUME":/var/lib/pgbackrest:ro \
  -v "$SECRETS/pgbackrest_cipher_pass":/run/cipher:ro \
  --entrypoint sh "$IMAGE" -c '
    set -e
    install -d -o postgres -g postgres -m 0700 /var/lib/postgresql/18/docker
    printf "[global]\nrepo1-path=/var/lib/pgbackrest\nrepo1-cipher-type=aes-256-cbc\nrepo1-cipher-pass=%s\nlock-path=/tmp/pgbackrest\nlog-path=/tmp\n[sin-barreras]\npg1-path=/var/lib/postgresql/18/docker\n" "$(cat /run/cipher)" > /etc/pgbackrest/pgbackrest.conf
    chown postgres /etc/pgbackrest/pgbackrest.conf
    gosu postgres pgbackrest --stanza=sin-barreras --type=immediate --target-action=promote restore
  ' | tail -2 | sed 's/^/    /'

echo "▶ 3/4 Arranque del Postgres restaurado (sin archivado)"
docker run -d --name "$NAME" --network "$NET" \
  -v "$VOLUME":/var/lib/postgresql \
  -v "$REPO_VOLUME":/var/lib/pgbackrest:ro \
  -v "$SECRETS/pgbackrest_cipher_pass":/run/cipher:ro \
  -v "$CERTS/ca.crt":/run/tls-src/ca.crt:ro \
  -v "$CERTS/postgres.crt":/run/tls-src/postgres.crt:ro \
  -v "$CERTS/postgres.key":/run/tls-src/postgres.key:ro \
  --entrypoint sh "$IMAGE" -c '
    install -d -o postgres -g postgres -m 0700 /etc/postgresql/tls
    install -o postgres -g postgres -m 0600 /run/tls-src/postgres.key /etc/postgresql/tls/server.key
    install -o postgres -g postgres -m 0644 /run/tls-src/postgres.crt /etc/postgresql/tls/server.crt
    install -o postgres -g postgres -m 0644 /run/tls-src/ca.crt /etc/postgresql/tls/ca.crt
    printf "[global]\nrepo1-path=/var/lib/pgbackrest\nrepo1-cipher-type=aes-256-cbc\nrepo1-cipher-pass=%s\nlock-path=/tmp/pgbackrest\nlog-path=/tmp\n[sin-barreras]\npg1-path=/var/lib/postgresql/18/docker\n" "$(cat /run/cipher)" > /etc/pgbackrest/pgbackrest.conf
    chown postgres /etc/pgbackrest/pgbackrest.conf
    exec docker-entrypoint.sh postgres -c config_file=/etc/postgresql/postgresql.conf -c archive_mode=off
  ' >/dev/null
for _ in $(seq 1 60); do
  if docker exec "$NAME" psql -U postgres -d sin_barreras -tAc "SELECT NOT pg_is_in_recovery()" 2>/dev/null | grep -q t; then break; fi
  sleep 1
done
RESTORED_COUNT=$(docker exec "$NAME" psql -U postgres -d sin_barreras -tAc "SELECT count(*) FROM registry.records")
echo "    registros restaurados: $RESTORED_COUNT"

echo "▶ 4/4 Verificación de integridad del registro restaurado (rol sb_auditor)"
docker run --rm --network "$NET" \
  -e PG_HOST="$NAME" -e PG_DATABASE=sin_barreras -e PG_USER=sb_auditor \
  -e PG_PASSWORD_FILE=/run/pw -e PG_CA_FILE=/run/ca.crt \
  -v "$SECRETS/postgres_auditor_password":/run/pw:ro \
  -v "$CERTS/ca.crt":/run/ca.crt:ro \
  "$ADMIN_IMAGE" verify | sed 's/^/    /'

if [ "$RESTORED_COUNT" -lt "$SOURCE_COUNT" ]; then
  echo "✗ Faltan registros: $RESTORED_COUNT de $SOURCE_COUNT"
  exit 1
fi
echo "✓ PRUEBA DE RESTAURACIÓN SUPERADA: respaldo cifrado restaurable, íntegro y completo."
