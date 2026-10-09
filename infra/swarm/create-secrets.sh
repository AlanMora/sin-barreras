#!/usr/bin/env bash
# Crea los Docker secrets de producción a partir de un directorio con un archivo
# por secreto (generado en un equipo seguro). Nunca guardar ese directorio en
# los servidores después de crear los secrets.
#
#   infra/swarm/create-secrets.sh /ruta/segura/secrets
set -euo pipefail
dir="${1:?uso: create-secrets.sh <directorio con un archivo por secreto>}"
required=(
  internal_ca valkey_tls_crt valkey_tls_key valkey_users_acl valkey_healthcheck_password
  valkey_bff_password valkey_record_writer_password postgres_tls_crt postgres_tls_key
  postgres_superuser_password postgres_migrator_password postgres_record_writer_password
  postgres_auditor_password postgres_reporting_password postgres_investigator_password
  postgres_replicator_password pgbackrest_cipher_pass pgbackrest_repo2_cipher_pass
  pgbackrest_s3_key pgbackrest_s3_secret c5_api_key qr_keys record_public_key
  record_operational_keys grafana_admin_password
)
for name in "${required[@]}"; do
  file="$dir/$name"
  [[ -s "$file" ]] || { echo "✗ falta $file"; exit 1; }
  if docker secret inspect "$name" >/dev/null 2>&1; then
    echo "· $name ya existe (para rotarlo: crear ${name}_v2 y actualizar el stack)"
  else
    docker secret create "$name" "$file" >/dev/null && echo "✓ $name"
  fi
done
# La llave PRIVADA de investigación NO es un secret del stack: vive fuera de línea.
echo "Listo. Recuerda: la llave privada de investigación nunca se carga en Swarm."
