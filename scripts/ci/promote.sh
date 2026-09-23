#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=migrations.sh
source "$root/scripts/ci/migrations.sh"

promote_name_ok() {
  [[ "$1" =~ ^env_promote_(staging|production)$ ]]
}

admin_exec() {
  PGDATABASE=postgres psql_exec "$@"
}

drop_promote_db() {
  local name=$1
  if ! promote_name_ok "$name"; then
    printf 'recuso remover banco fora do ensaio de promoção\n' >&2
    return 1
  fi
  admin_exec --command "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${name}' AND pid <> pg_backend_pid();" >/dev/null
  admin_exec --command "DROP DATABASE IF EXISTS ${name};"
}

create_promote_db() {
  local name=$1
  promote_name_ok "$name" || return 1
  drop_promote_db "$name"
  admin_exec --command "CREATE DATABASE ${name};"
}

db_exists() {
  local name=$1 found
  [[ "$name" =~ ^[a-z_]+$ ]] || return 1
  found=$(admin_exec -tA --command "SELECT 1 FROM pg_database WHERE datname = '${name}'")
  [[ "$found" == "1" ]]
}

require_connection() {
  if [[ -z "${PGHOST:-}" || -z "${PGUSER:-}" || -z "${PGDATABASE:-}" || -z "${PGPASSWORD:-}" ]]; then
    printf 'conexão de promoção incompleta\n' >&2
    return 1
  fi
}

promote_real() {
  local target=${AUTO_PLATFORM_PROMOTION_TARGET:-}
  if [[ "${AUTO_PLATFORM_CI_DATABASE:-}" == "1" ]]; then
    printf 'recuso promoção real com banco de CI\n' >&2
    return 1
  fi
  if [[ "$target" != "staging" && "$target" != "production" ]]; then
    printf 'alvo de promoção inválido\n' >&2
    return 1
  fi
  if [[ "${RUN_MIGRATIONS_ON_STARTUP:-false}" != "false" ]]; then
    printf 'recuso promoção com migration no startup\n' >&2
    return 1
  fi
  require_connection
  apply_dir "$migrations"
  printf 'migrations do alvo %s aplicadas\n' "$target"
}

run_promote_real() {
  local target=$1 database=$2 startup=$3 confirmation=$4 ci_flag=$5 event_name=$6
  (
    if [[ -z "$ci_flag" ]]; then
      unset AUTO_PLATFORM_CI_DATABASE
    else
      export AUTO_PLATFORM_CI_DATABASE="$ci_flag"
    fi
    export GITHUB_EVENT_NAME="$event_name"
    export AUTO_PLATFORM_PROMOTION_TARGET="$target"
    export AUTO_PLATFORM_PROMOTION_CONFIRMATION="$confirmation"
    export RUN_MIGRATIONS_ON_STARTUP="$startup"
    export PGDATABASE="$database"
    promote_real
  )
}

expect_message() {
  local output=$1 needle=$2
  [[ "$output" == *"$needle"* ]] || {
    printf 'promoção não recusou pelo motivo esperado\n' >&2
    return 1
  }
}

prove_and_drill() {
  local name output marker phrase target
  if [[ "${AUTO_PLATFORM_CI_DATABASE:-}" != "1" ]]; then
    printf 'ensaio de promoção exige banco descartável\n' >&2
    exit 1
  fi
  for name in production postgres autoplatform env_drill_staging; do
    if drop_promote_db "$name" >/dev/null 2>&1; then
      printf 'promoção removeu banco fora do padrão\n' >&2
      exit 1
    fi
  done

  output=$(run_promote_real staging env_promote_staging false "nao-aprovo" "" "" 2>&1 || true)
  expect_message "$output" "recuso aplicar migration sem banco descartável ou promoção explícita do alvo"
  if db_exists env_promote_staging; then
    printf 'confirmação inválida criou banco\n' >&2
    exit 1
  fi

  marker=$(printf '%s%s' 'synthetic-' 'db-pass')
  output=$(
    PGPASSWORD="$marker" run_promote_real staging env_promote_staging false "nao-aprovo" "" "" 2>&1 || true
  )
  if [[ "$output" == *"$marker"* || "$output" == *"nao-aprovo"* ]]; then
    printf 'log de promoção expôs credencial ou confirmação\n' >&2
    exit 1
  fi

  output=$(run_promote_real production env_promote_production true "aprovo-migrations-production" "" "" 2>&1 || true)
  expect_message "$output" "recuso promoção com migration no startup"
  if [[ "$output" == *"aprovo-migrations-production"* ]]; then
    printf 'log de promoção expôs a confirmação\n' >&2
    exit 1
  fi

  output=$(run_promote_real staging env_promote_staging false "aprovo-migrations-staging" "1" "workflow_dispatch" 2>&1 || true)
  expect_message "$output" "recuso promoção real com banco de CI"

  output=$(
    unset PGPASSWORD
    run_promote_real staging env_promote_staging false "aprovo-migrations-staging" "" "" 2>&1 || true
  )
  expect_message "$output" "conexão de promoção incompleta"
  if [[ "$output" == *"aprovo-migrations-staging"* ]]; then
    printf 'log de promoção expôs a confirmação\n' >&2
    exit 1
  fi

  for target in staging production; do
    name="env_promote_${target}"
    phrase="aprovo-migrations-${target}"
    create_promote_db "$name"
    output=$(run_promote_real "$target" "$name" false "$phrase" "" "" 2>&1)
    [[ "$output" == "migrations do alvo ${target} aplicadas" ]]
    if [[ "$output" == *"$phrase"* ]]; then
      printf 'log de promoção expôs a confirmação\n' >&2
      exit 1
    fi
    (
      export PGDATABASE="$name"
      apply_dir "$migrations"
    )
    drop_promote_db "$name"
  done
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  if [[ "${AUTO_PLATFORM_CI_DATABASE:-}" == "1" ]]; then
    prove_and_drill
  else
    promote_real
  fi
fi
