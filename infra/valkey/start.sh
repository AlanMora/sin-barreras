#!/bin/sh
# Arranque de Valkey: copia el material TLS y la ACL con el dueño correcto y
# delega en el entrypoint oficial (que baja privilegios al usuario `valkey`).
# Se ejecuta como root dentro del contenedor.
set -eu
install -d -o valkey -g valkey -m 0700 /tls
install -o valkey -g valkey -m 0600 /run/tls-src/valkey.key /tls/valkey.key
install -o valkey -g valkey -m 0644 /run/tls-src/valkey.crt /tls/valkey.crt
install -o valkey -g valkey -m 0644 /run/tls-src/ca.crt /tls/ca.crt
install -o valkey -g valkey -m 0600 /run/secrets/valkey_users_acl /tls/users.acl
exec docker-entrypoint.sh valkey-server /usr/local/etc/valkey/valkey.conf
