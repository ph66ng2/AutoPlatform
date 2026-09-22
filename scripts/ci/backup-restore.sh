#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=migrations.sh
source "$root/scripts/ci/migrations.sh"

if [[ "${AUTO_PLATFORM_CI_DATABASE:-}" != "1" ]]; then
  printf 'ensaio de backup exige banco descartável\n' >&2
  exit 1
fi

work=$(mktemp -d)
cleanup() {
  local status=$? name
  for name in env_drill_staging env_drill_production env_drill_staging_restored; do
    drop_drill_db "$name" >/dev/null 2>&1 || true
  done
  rm -rf "$work"
  exit "$status"
}
trap cleanup EXIT

drill_name_ok() {
  [[ "$1" =~ ^env_drill_[a-z_]+$ ]]
}

admin_exec() {
  PGDATABASE=postgres psql_exec "$@"
}

drop_drill_db() {
  local name=$1
  if ! drill_name_ok "$name"; then
    printf 'recuso remover banco fora do ensaio de backup\n' >&2
    return 1
  fi
  admin_exec --command "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${name}' AND pid <> pg_backend_pid();" >/dev/null
  admin_exec --command "DROP DATABASE IF EXISTS ${name};"
}

create_drill_db() {
  local name=$1
  drill_name_ok "$name" || return 1
  drop_drill_db "$name"
  admin_exec --command "CREATE DATABASE ${name};"
}

read_backup_environment() {
  local meta=$1 line
  [[ -f "$meta" ]] || return 1
  line=$(tr -d '\r' <"$meta")
  [[ "$line" =~ ^environment=(staging|production)$ ]] || return 1
  printf '%s' "${BASH_REMATCH[1]}"
}

restore_dump() {
  local dump=$1 dest=$2 expected=$3 actual
  if [[ "$expected" != "staging" && "$expected" != "production" ]]; then
    printf 'ambiente de restore inválido\n' >&2
    return 1
  fi
  if [[ "$dest" != "env_drill_${expected}" && "$dest" != "env_drill_${expected}_restored" ]]; then
    printf 'destino não pertence ao ambiente %s\n' "$expected" >&2
    return 1
  fi
  if ! actual=$(read_backup_environment "${dump}.meta"); then
    printf 'backup sem ambiente de origem\n' >&2
    return 1
  fi
  if [[ "$actual" != "$expected" ]]; then
    printf 'recuso restaurar backup de %s no ambiente %s\n' "$actual" "$expected" >&2
    return 1
  fi
  hash -r
  command pg_restore --no-owner --no-acl --dbname "$dest" "$dump"
}

dump_env() {
  local name=$1 env_name=$2 dest=$3
  drill_name_ok "$name" || return 1
  [[ "$env_name" == "staging" || "$env_name" == "production" ]] || return 1
  pg_dump --format=custom --no-owner --no-acl --file="$dest" --dbname="$name"
  printf 'environment=%s\n' "$env_name" >"${dest}.meta"
}

ledger() {
  local name=$1
  PGDATABASE="$name" psql_exec -tA --command "SELECT version || ':' || checksum FROM platform.schema_migrations ORDER BY version;"
}

marker() {
  local name=$1
  PGDATABASE="$name" psql_exec -tA --command "SELECT environment FROM platform.backup_marker;"
}

mark_env() {
  local name=$1 env_name=$2
  [[ "$env_name" == "staging" || "$env_name" == "production" ]] || return 1
  PGDATABASE="$name" psql_exec --command "CREATE TABLE platform.backup_marker (environment text PRIMARY KEY);"
  PGDATABASE="$name" psql_exec --command "INSERT INTO platform.backup_marker (environment) VALUES ('${env_name}');"
}

platform_count() {
  local name=$1
  PGDATABASE="$name" psql_exec -tA --command "SELECT count(*) FROM pg_namespace WHERE nspname = 'platform';"
}

prove_drop_refuses_other_names() {
  local name
  for name in production postgres autoplatform env_promote_staging; do
    if drop_drill_db "$name" >/dev/null 2>&1; then
      printf 'ensaio removeu banco fora do padrão\n' >&2
      exit 1
    fi
  done
  [[ "$(admin_exec -tA --command "SELECT datname FROM pg_database WHERE datname = 'autoplatform';")" == "autoplatform" ]]
}

prove_cross_restore_skips_pg_restore() {
  local bin dump err_file
  bin=$(mktemp -d)
  printf '#!/bin/sh\nprintf "pg_restore foi chamado\\n" >&2\nexit 99\n' >"$bin/pg_restore"
  chmod +x "$bin/pg_restore"
  dump="$work/negative.dump"
  printf 'environment=staging\n' >"${dump}.meta"
  err_file="$work/negative.err"
  if PATH="$bin:$PATH" restore_dump "$dump" env_drill_production production >"$work/negative.out" 2>"$err_file"; then
    printf 'restore cruzado foi aceito\n' >&2
    exit 1
  fi
  if grep -q 'pg_restore foi chamado' "$err_file"; then
    printf 'restore cruzado chamou pg_restore\n' >&2
    exit 1
  fi
  rm -rf "$bin"
  hash -r
}

apply_named() {
  local name=$1
  (
    export PGDATABASE="$name"
    apply_dir "$migrations"
  )
}

prove_drop_refuses_other_names
prove_cross_restore_skips_pg_restore

create_drill_db env_drill_staging
create_drill_db env_drill_production
apply_named env_drill_staging
apply_named env_drill_production
mark_env env_drill_staging staging
mark_env env_drill_production production

staging_ledger=$(ledger env_drill_staging)
production_ledger=$(ledger env_drill_production)
staging_marker=$(marker env_drill_staging)
production_marker=$(marker env_drill_production)
[[ "$staging_marker" == "staging" ]]
[[ "$production_marker" == "production" ]]

dump_env env_drill_staging staging "$work/staging.dump"
dump_env env_drill_production production "$work/production.dump"

if restore_dump "$work/staging.dump" env_drill_production production >/dev/null 2>&1; then
  printf 'backup de staging entrou em produção\n' >&2
  exit 1
fi
[[ "$(ledger env_drill_production)" == "$production_ledger" ]]
[[ "$(marker env_drill_production)" == "$production_marker" ]]
[[ "$(ledger env_drill_staging)" == "$staging_ledger" ]]

create_drill_db env_drill_staging_restored
if restore_dump "$work/production.dump" env_drill_staging_restored staging >/dev/null 2>&1; then
  printf 'backup de produção entrou em staging\n' >&2
  exit 1
fi
[[ "$(platform_count env_drill_staging_restored)" == "0" ]]

restore_dump "$work/staging.dump" env_drill_staging_restored staging >/dev/null
[[ "$(ledger env_drill_staging_restored)" == "$staging_ledger" ]]
[[ "$(marker env_drill_staging_restored)" == "staging" ]]
