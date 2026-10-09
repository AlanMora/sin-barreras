#!/bin/sh
# Crea los roles de Sin Barreras al inicializar el clúster (solo la primera vez).
# Las contraseñas vienen de Docker secrets; nunca aparecen en la línea de comandos
# de otros procesos porque psql las recibe como variables desde este script.
set -eu
secret() { cat "/run/secrets/postgres_$1_password"; }
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
  -v dbname="$POSTGRES_DB" \
  -v migrator_password="$(secret migrator)" \
  -v writer_password="$(secret record_writer)" \
  -v auditor_password="$(secret auditor)" \
  -v reporting_password="$(secret reporting)" \
  -v investigator_password="$(secret investigator)" \
  -v replicator_password="$(secret replicator)" \
  -f /docker-entrypoint-initdb.d/roles.sql.inc
