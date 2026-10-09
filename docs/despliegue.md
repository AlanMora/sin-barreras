# Despliegue en Docker Swarm

## Topología mínima recomendada (3 nodos)

| Nodo | Etiquetas | Servicios fijos | Además (Swarm reparte) |
|---|---|---|---|
| A (manager) | `sb.edge=true`, `sb.postgres=primary` | Traefik, Postgres primario + pgBackRest | BFF, record-writer, PWA |
| B (manager) | `sb.edge=true`, `sb.postgres=replica` | Traefik, Postgres réplica | BFF, record-writer, PWA |
| C (manager) | `sb.valkey=true` | Valkey | observabilidad |

Tres managers para el quórum de Swarm. BFF y record-writer tienen
`max_replicas_per_node: 1`: siempre quedan en nodos distintos.

## Preparación de los nodos

```bash
docker swarm init                      # en A; unir B y C como managers
docker node update --label-add sb.edge=true --label-add sb.postgres=primary nodo-a
docker node update --label-add sb.edge=true --label-add sb.postgres=replica nodo-b
docker node update --label-add sb.valkey=true nodo-c
# Nodo de Valkey (ADR-0003): sin esto la reescritura del AOF puede fallar.
echo 'vm.overcommit_memory = 1' | sudo tee /etc/sysctl.d/99-valkey.conf && sudo sysctl --system
```

- Discos: SSD local para Valkey (`appendfsync always`, ADR-0015) y Postgres.
- Disco cifrado (LUKS) en los nodos de Valkey y Postgres: Valkey guarda en claro
  el estado operativo de las sesiones activas (ver `docs/modelo-datos.md`).
- NTP sincronizado en todos los nodos (marcas de tiempo del registro).
- Firewall: solo 80/443 públicos; puertos de Swarm (2377, 7946, 4789) solo
  entre nodos. Las redes `backend` y `obs` van cifradas (IPsec).

## Secrets

1. En un equipo seguro, generar llaves: `docker run --rm sin-barreras/sb-admin keygen inv-2026`
   (ver `docs/llaves.md`). La **privada** se guarda fuera de línea.
2. Certificados internos (CA propia) para Valkey y Postgres.
3. `infra/swarm/create-secrets.sh /ruta/segura` crea los 25 secrets.

## Despliegue

```bash
export REGISTRY=registro.zapopan.gob.mx TAG=1.0.0
export DOMAIN=sinbarreras.zapopan.gob.mx ACME_EMAIL=c5@zapopan.gob.mx
export OPS_DOMAIN=ops.sinbarreras.zapopan.gob.mx OPS_ALLOWLIST=10.20.0.0/16
export BACKUP_S3_BUCKET=sb-respaldos BACKUP_S3_ENDPOINT=https://s3.otro-sitio.example
docker stack deploy -c infra/swarm/docker-stack.yml --with-registry-auth sin-barreras
```

- Las actualizaciones son `start-first` con rollback automático: cero caída.
- Si cambia el protocolo PWA↔BFF, desplegar **primero el BFF**
  (`docker service update --image … sin-barreras_bff`) y después la PWA.
- `migrate` corre como job en cada despliegue (idempotente).

## Verificación posterior

```bash
docker service ls                                   # todo en N/N
docker run --rm --network sin-barreras_backend … sb-admin verify   # integridad
node scripts/smoke-ws.mjs wss://$DOMAIN/ws          # punta a punta
```

## Pendiente antes de producción

- Adaptador real del C5 (hoy `c5-mock`; quitarlo del stack).
- Alta disponibilidad de Valkey (hoy un nodo con AOF + reinicio automático;
  evaluar réplica + Sentinel, soportado por el cliente `fred`).
- Receptor real de Alertmanager (correo / mesa de servicio).
- Prueba de despliegue en el clúster de pruebas del municipio.
