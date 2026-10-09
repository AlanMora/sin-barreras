# Gestión de llaves

| Llave | Dónde vive | Quién la usa | Si se compromete |
|---|---|---|---|
| Pública de investigación (`RECORD_PUBLIC_KEY`) | Docker secret del BFF | BFF (cifrar) | Nada: es pública |
| **Privada de investigación** | **Fuera de línea** (bóveda/HSM o USB cifrada en caja fuerte, con copia) | `sb-admin` en investigaciones | Todo el historial queda expuesto (si además hay acceso a la base). Rotar de inmediato |
| Operativa (`RECORD_OPERATIONAL_KEYS`) | Docker secret del BFF | BFF (compartir DEK entre réplicas) | Con un volcado de Valkey se leerían las DEK de las sesiones activas. Rotar |
| Firma de QR (`QR_KEYS`) | Docker secret del BFF | BFF (verificar QR) | Se podrían fabricar QR "verificados". Rotar y reimprimir (ADR-0007: no bloquea la ayuda) |
| Contraseñas de Postgres y Valkey | Docker secrets | Cada servicio con su rol | Rotar la del rol afectado |

## Generación (producción)

```bash
# En un equipo fuera de línea:
docker run --rm sin-barreras/sb-admin keygen inv-2026
```

1. Guarda la línea "PRIVADA" solo en la bóveda (dos custodios, dos copias).
2. Crea los secrets de Swarm con la pública, la operativa y la de QR:
   `printf '%s' '<valor>' | docker secret create record_public_key_v1 -`

## Rotación de la llave de investigación

1. Genera un par nuevo (`inv-2027`).
2. Crea el secret nuevo y actualiza el servicio BFF para usarlo (despliegue
   escalonado). Las sesiones nuevas usan la llave nueva.
3. Conserva la privada anterior: la necesitan las sesiones previas.
4. (Opcional) Re-envuelve las DEK antiguas con la llave nueva y destruye la
   privada anterior. Se insertan filas nuevas en `registry.session_keys`; nada
   se modifica.

## Rotación de la llave operativa

`RECORD_OPERATIONAL_KEYS=op2:<nueva>,op1:<anterior>` → despliegue → tras 12 h
(máximo TTL de sesión), quitar `op1`.

## En desarrollo

`infra/dev/bootstrap.sh` genera todas las llaves en `infra/dev/secrets/`
(incluida la privada, para probar la investigación). Nunca se usan fuera de tu
máquina.
