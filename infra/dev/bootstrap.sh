#!/usr/bin/env bash
# Genera certificados TLS y secretos para el entorno LOCAL de desarrollo.
#
# - Una CA de desarrollo propia y certificados de servidor para Valkey y Postgres.
# - Contraseñas aleatorias por servicio y el archivo de ACL de Valkey con hashes
#   SHA-256 (las contraseñas en claro nunca entran al archivo de ACL).
#
# Todo se escribe en infra/dev/certs y infra/dev/secrets, que están en
# .gitignore. NUNCA uses estos archivos fuera de tu máquina.
#
# Uso: infra/dev/bootstrap.sh [--force]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
certs="$root/certs"
secrets="$root/secrets"

if [[ "${1:-}" == "--force" ]]; then
  rm -rf "$certs" "$secrets"
fi
if [[ -f "$certs/ca.crt" && -f "$secrets/valkey-users.acl" ]]; then
  echo "✓ Certificados y secretos de desarrollo ya existen (usa --force para regenerarlos)."
  exit 0
fi
umask 077
mkdir -p "$certs" "$secrets"

echo "→ CA de desarrollo"
openssl ecparam -name prime256v1 -genkey -noout -out "$certs/ca.key"
openssl req -x509 -new -key "$certs/ca.key" -sha256 -days 825 \
  -subj "/O=Sin Barreras DEV/CN=Sin Barreras Dev CA" -out "$certs/ca.crt"

server_cert() { # nombre, SANs
  local name="$1" sans="$2"
  echo "→ Certificado de servidor: $name"
  openssl ecparam -name prime256v1 -genkey -noout -out "$certs/$name.key"
  openssl req -new -key "$certs/$name.key" -subj "/O=Sin Barreras DEV/CN=$name" -out "$certs/$name.csr"
  openssl x509 -req -in "$certs/$name.csr" -CA "$certs/ca.crt" -CAkey "$certs/ca.key" \
    -CAcreateserial -days 825 -sha256 -out "$certs/$name.crt" \
    -extfile <(printf "subjectAltName=%s\nextendedKeyUsage=serverAuth\nkeyUsage=digitalSignature" "$sans")
  rm "$certs/$name.csr"
}
server_cert valkey "DNS:valkey,DNS:localhost,IP:127.0.0.1"
server_cert postgres "DNS:postgres,DNS:localhost,IP:127.0.0.1"
rm -f "$certs/ca.srl"

echo "→ Contraseñas de desarrollo"
password() { openssl rand -hex 24; }
for user in admin bff record_writer healthcheck; do
  password > "$secrets/valkey_${user}_password"
done
password > "$secrets/postgres_superuser_password"

echo "→ ACL de Valkey"
hash() { printf '%s' "$(cat "$1")" | openssl dgst -sha256 -r | cut -d' ' -f1; }
# Mínimo privilegio: el usuario `default` queda deshabilitado. Los permisos de
# bff y record_writer se acotan por prefijo de llaves y se afinan en las
# Fases 1 y 2, cuando existan los streams y llaves definitivas.
cat > "$secrets/valkey-users.acl" <<ACL
user default off
user admin on #$(hash "$secrets/valkey_admin_password") ~* &* +@all
user healthcheck on #$(hash "$secrets/valkey_healthcheck_password") resetkeys -@all +ping +hello
user bff on #$(hash "$secrets/valkey_bff_password") ~sb:* &sb:* +@all -@dangerous -@admin
user record_writer on #$(hash "$secrets/valkey_record_writer_password") ~sb:events* -@all +@stream +@connection +ping
ACL

# Permisos:
# - Las carpetas quedan en 0700: ningún otro usuario del host puede entrar.
# - Las llaves privadas TLS quedan en 0600: los contenedores las copian como
#   root con el dueño correcto antes de arrancar el servidor.
# - Los secretos quedan en 0644 porque Docker Compose (fuera de Swarm) monta
#   los secrets con los permisos del host e ignora uid/gid/mode, y los
#   servicios corren con usuarios no-root. En Swarm se usan uid/gid/mode 0400.
chmod 700 "$certs" "$secrets"
chmod 600 "$certs"/*.key
chmod 644 "$certs"/*.crt "$secrets"/*
echo "✓ Listo: $certs y $secrets"
