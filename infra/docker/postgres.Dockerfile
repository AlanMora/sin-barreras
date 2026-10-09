# syntax=docker/dockerfile:1.7
# Postgres 18 con pgBackRest (respaldos completos/incrementales cifrados y
# archivado continuo de WAL) y los scripts de arranque de Sin Barreras.
FROM postgres:18-alpine
RUN apk add --no-cache pgbackrest \
    && install -d -o postgres -g postgres -m 0750 /var/lib/pgbackrest /var/log/pgbackrest /var/spool/pgbackrest \
    && install -d -o postgres -g postgres -m 0755 /etc/postgresql/conf.d
COPY infra/postgres/start.sh /usr/local/bin/sb-start.sh
COPY infra/postgres/postgresql.conf /etc/postgresql/postgresql.conf
COPY infra/postgres/pg_hba.conf /etc/postgresql/pg_hba.conf
COPY infra/postgres/pg_ident.conf /etc/postgresql/pg_ident.conf
COPY infra/postgres/backup-crontab /etc/crontabs/root
COPY infra/postgres/initdb/10-roles.sh /docker-entrypoint-initdb.d/10-roles.sh
COPY crates/registry/sql/roles.sql /docker-entrypoint-initdb.d/roles.sql.inc
ENTRYPOINT ["/bin/sh", "/usr/local/bin/sb-start.sh"]
