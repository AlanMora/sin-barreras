#!/usr/bin/env bash
# Genera certificados TLS y secretos para el entorno LOCAL de desarrollo.
#
# - Una CA de desarrollo propia y certificados de servidor para Valkey y Postgres.
# - Contraseñas aleatorias por servicio y el archivo de ACL de Valkey con hashes
#   SHA-256 (las contraseñas en claro nunca entran al archivo de ACL).
# - Llaves de la aplicación (firma de QR, API key del simulador del C5…).
#
# Es idempotente: solo crea lo que falta. Con --force regenera todo.
# Todo se escribe en infra/dev/certs y infra/dev/secrets (en .gitignore).
# NUNCA uses estos archivos fuera de tu máquina.
#
# Uso: infra/dev/bootstrap.sh [--force]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
certs="$root/certs"
secrets="$root/secrets"

if [[ "${1:-}" == "--force" ]]; then
  rm -rf "$certs" "$secrets"
fi
umask 077
mkdir -p "$certs" "$secrets"

# --- TLS ----------------------------------------------------------------
if [[ ! -f "$certs/ca.crt" ]]; then
  echo "→ CA de desarrollo"
  openssl ecparam -name prime256v1 -genkey -noout -out "$certs/ca.key"
  openssl req -x509 -new -key "$certs/ca.key" -sha256 -days 825 \
    -subj "/O=Sin Barreras DEV/CN=Sin Barreras Dev CA" -out "$certs/ca.crt"
  rm -f "$certs"/valkey.* "$certs"/postgres.*
fi

server_cert() { # nombre, SANs
  local name="$1" sans="$2"
  [[ -f "$certs/$name.crt" ]] && return
  echo "→ Certificado de servidor: $name"
  openssl ecparam -name prime256v1 -genkey -noout -out "$certs/$name.key"
  openssl req -new -key "$certs/$name.key" -subj "/O=Sin Barreras DEV/CN=$name" -out "$certs/$name.csr"
  openssl x509 -req -in "$certs/$name.csr" -CA "$certs/ca.crt" -CAkey "$certs/ca.key" \
    -CAcreateserial -days 825 -sha256 -out "$certs/$name.crt" \
    -extfile <(printf "subjectAltName=%s\nextendedKeyUsage=serverAuth\nkeyUsage=digitalSignature" "$sans") 2>/dev/null
  rm -f "$certs/$name.csr" "$certs/ca.srl"
}
server_cert valkey "DNS:valkey,DNS:localhost,IP:127.0.0.1"
server_cert postgres "DNS:postgres,DNS:postgres-replica,DNS:localhost,IP:127.0.0.1"

# --- Secretos -------------------------------------------------------------
new_secret() { # archivo, comando que imprime el valor
  local file="$secrets/$1"
  shift
  [[ -f "$file" ]] && return
  echo "→ Secreto: $(basename "$file")"
  "$@" > "$file"
}
hex() { openssl rand -hex 24; }
qr_keys() { printf '1:%s' "$(openssl rand -base64 32)"; }

for user in admin bff record_writer healthcheck; do
  new_secret "valkey_${user}_password" hex
done
for role in superuser migrator record_writer auditor reporting investigator replicator; do
  new_secret "postgres_${role}_password" hex
done
new_secret c5_mock_api_key hex
new_secret qr_keys qr_keys
new_secret record_operational_keys sh -c "printf 'op1:%s' \"\$(openssl rand -base64 32)\""

# Par de llaves de investigación (X25519). En desarrollo la privada vive
# aquí para poder probar la investigación; en producción NUNCA está en los
# servidores en línea (ver docs/llaves.md).
if [[ ! -f "$secrets/record_public_key" ]]; then
  echo "→ Llaves de investigación del registro"
  key="$(mktemp)"
  openssl genpkey -algorithm X25519 -out "$key" 2>/dev/null
  printf 'dev1:%s' "$(openssl pkey -in "$key" -outform DER | tail -c 32 | base64 -w0)" > "$secrets/record_private_key"
  printf 'dev1:%s' "$(openssl pkey -in "$key" -pubout -outform DER | tail -c 32 | base64 -w0)" > "$secrets/record_public_key"
  rm -f "$key"
fi

# El ACL se regenera siempre a partir de las contraseñas (es derivado).
hash() { printf '%s' "$(cat "$1")" | openssl dgst -sha256 -r | cut -d' ' -f1; }
# Mínimo privilegio: el usuario `default` queda deshabilitado.
# - bff: solo llaves y canales `sb:*`, sin comandos peligrosos ni de administración
#   (excepto INFO, de solo lectura, que el cliente usa para detectar la versión).
# - record_writer: solo los streams del registro (`sb:rec*`), solo lectura de
#   streams y consumer groups.
cat > "$secrets/valkey-users.acl" <<ACL
user default off
user admin on #$(hash "$secrets/valkey_admin_password") ~* &* +@all
user healthcheck on #$(hash "$secrets/valkey_healthcheck_password") resetkeys -@all +ping +hello
user bff on #$(hash "$secrets/valkey_bff_password") ~sb:* &sb:* +@all -@dangerous -@admin +info
user record_writer on #$(hash "$secrets/valkey_record_writer_password") ~sb:rec* resetchannels -@all +@stream +@connection +ping +info
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
echo "✓ Certificados y secretos de desarrollo listos en infra/dev/"
