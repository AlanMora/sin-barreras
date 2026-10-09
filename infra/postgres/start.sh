#!/bin/sh
# Arranque de Postgres de Sin Barreras. Se ejecuta como root dentro del
# contenedor y delega en el entrypoint oficial (que baja al usuario postgres).
#
#   SB_PG_ROLE=primary  (por defecto) primario: archiva WAL y hace respaldos.
#   SB_PG_ROLE=replica  réplica de streaming: se inicializa sola desde el
#                        primario (SB_PG_PRIMARY_HOST) si su volumen está vacío.
set -eu
ROLE="${SB_PG_ROLE:-primary}"
PGDATA="${PGDATA:-/var/lib/postgresql/18/docker}"

# --- Material TLS con el dueño y permisos que exige Postgres ---------------
install -d -o postgres -g postgres -m 0700 /etc/postgresql/tls
install -o postgres -g postgres -m 0600 /run/tls-src/postgres.key /etc/postgresql/tls/server.key
install -o postgres -g postgres -m 0644 /run/tls-src/postgres.crt /etc/postgresql/tls/server.crt
install -o postgres -g postgres -m 0644 /run/tls-src/ca.crt /etc/postgresql/tls/ca.crt

# --- pgBackRest: configuración generada desde secrets ----------------------
if [ -f /run/secrets/pgbackrest_cipher_pass ]; then
  install -d -o postgres -g postgres -m 0750 /etc/pgbackrest
  {
    echo "[global]"
    echo "repo1-path=/var/lib/pgbackrest"
    echo "repo1-retention-full=${PGBACKREST_RETENTION_FULL:-4}"
    echo "repo1-cipher-type=aes-256-cbc"
    echo "repo1-cipher-pass=$(cat /run/secrets/pgbackrest_cipher_pass)"
    if [ -n "${PGBACKREST_REPO2_S3_BUCKET:-}" ]; then
      # Copia fuera del servidor (S3 compatible), con su propia llave.
      echo "repo2-type=s3"
      echo "repo2-path=/sin-barreras"
      echo "repo2-s3-bucket=${PGBACKREST_REPO2_S3_BUCKET}"
      echo "repo2-s3-endpoint=${PGBACKREST_REPO2_S3_ENDPOINT}"
      echo "repo2-s3-region=${PGBACKREST_REPO2_S3_REGION:-us-east-1}"
      echo "repo2-s3-key=$(cat /run/secrets/pgbackrest_s3_key)"
      echo "repo2-s3-key-secret=$(cat /run/secrets/pgbackrest_s3_secret)"
      echo "repo2-retention-full=${PGBACKREST_RETENTION_FULL:-4}"
      echo "repo2-cipher-type=aes-256-cbc"
      echo "repo2-cipher-pass=$(cat /run/secrets/pgbackrest_repo2_cipher_pass)"
    fi
    echo "compress-type=zst"
    echo "start-fast=y"
    echo "archive-async=y"
    echo "spool-path=/var/spool/pgbackrest"
    echo "log-level-console=info"
    echo "log-level-file=info"
    echo ""
    echo "[sin-barreras]"
    echo "pg1-path=${PGDATA}"
    echo "pg1-socket-path=/var/run/postgresql"
  } > /etc/pgbackrest/pgbackrest.conf
  chown postgres:postgres /etc/pgbackrest/pgbackrest.conf
  chmod 0600 /etc/pgbackrest/pgbackrest.conf
fi

if [ "$ROLE" = "replica" ]; then
  # --- Réplica: base inicial desde el primario si el volumen está vacío -----
  if [ ! -s "$PGDATA/PG_VERSION" ]; then
    install -d -o postgres -g postgres -m 0700 "$PGDATA"
    printf '%s:5432:replication:sb_replicator:%s\n' "${SB_PG_PRIMARY_HOST:-postgres}" \
      "$(cat /run/secrets/postgres_replicator_password)" > /var/lib/postgresql/.pgpass
    chown postgres:postgres /var/lib/postgresql/.pgpass
    chmod 0600 /var/lib/postgresql/.pgpass
    conn="host=${SB_PG_PRIMARY_HOST:-postgres} port=5432 user=sb_replicator sslmode=verify-full sslrootcert=/etc/postgresql/tls/ca.crt passfile=/var/lib/postgresql/.pgpass application_name=${SB_PG_REPLICA_NAME:-replica1}"
    echo "→ Inicializando réplica desde ${SB_PG_PRIMARY_HOST:-postgres}"
    until gosu postgres pg_isready -h "${SB_PG_PRIMARY_HOST:-postgres}" -q; do sleep 2; done
    gosu postgres pg_basebackup -d "$conn" -D "$PGDATA" -R -X stream -C -S "${SB_PG_REPLICA_NAME:-replica1}" \
      || gosu postgres pg_basebackup -d "$conn" -D "$PGDATA" -R -X stream -S "${SB_PG_REPLICA_NAME:-replica1}"
  fi
  exec docker-entrypoint.sh postgres -c config_file=/etc/postgresql/postgresql.conf -c hot_standby=on
fi

# --- Primario: archivado continuo de WAL, stanza de pgBackRest y respaldos
# programados (crond de busybox). ------------------------------------------
install -d -o postgres -g postgres -m 0755 /etc/postgresql/conf.d
if [ -f /etc/pgbackrest/pgbackrest.conf ]; then
  cat > /etc/postgresql/conf.d/archive.conf <<CONF
archive_mode = on
archive_command = 'pgbackrest --stanza=sin-barreras archive-push %p'
CONF
  (
    # Espera al servidor definitivo (el temporal de initdb no escucha por TCP).
    until gosu postgres pg_isready -h 127.0.0.1 -q; do sleep 2; done
    gosu postgres pgbackrest --stanza=sin-barreras stanza-create \
      && gosu postgres pgbackrest --stanza=sin-barreras check
  ) >> /var/log/pgbackrest/stanza.log 2>&1 &
  crond -b -L /var/log/pgbackrest/crond.log
fi
exec docker-entrypoint.sh postgres -c config_file=/etc/postgresql/postgresql.conf
