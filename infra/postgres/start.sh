#!/bin/sh
# Arranque de Postgres: copia el material TLS con el dueño y permisos que
# Postgres exige (llave privada 0600 propiedad de `postgres`) y delega en el
# entrypoint oficial. Se ejecuta como root dentro del contenedor.
set -eu
install -d -o postgres -g postgres -m 0700 /etc/postgresql/tls
install -o postgres -g postgres -m 0600 /run/tls-src/postgres.key /etc/postgresql/tls/server.key
install -o postgres -g postgres -m 0644 /run/tls-src/postgres.crt /etc/postgresql/tls/server.crt
install -o postgres -g postgres -m 0644 /run/tls-src/ca.crt /etc/postgresql/tls/ca.crt
exec docker-entrypoint.sh postgres -c config_file=/etc/postgresql/postgresql.conf
