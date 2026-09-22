#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
migrations="$root/migrations"

sql_files() {
  local dir=$1
  find "$dir" -maxdepth 1 -type f -name '*.sql' -printf '%f\n' | sort
}

check_dir() {
  local dir=$1
  local manifest="$dir/manifest.sha256"
  local files=()
  local base num prev=0 listed actual
  [[ -f "$manifest" ]] || {
    printf 'manifest de migration ausente em %s\n' "$dir" >&2
    return 1
  }
  while IFS= read -r base; do
    [[ -n "$base" ]] || continue
    files+=("$base")
  done < <(sql_files "$dir")
  if ((${#files[@]} == 0)); then
    printf 'nenhuma migration em %s\n' "$dir" >&2
    return 1
  fi
  for base in "${files[@]}"; do
    [[ "$base" =~ ^[0-9]{4}_[a-z0-9_]+\.sql$ ]] || {
      printf 'nome de migration inválido: %s\n' "$base" >&2
      return 1
    }
    num=$((10#${base:0:4}))
    if ((num != prev + 1)); then
      printf 'migration fora de sequência: %s\n' "$base" >&2
      return 1
    fi
    prev=$num
  done
  listed=$(awk '{print $2}' "$manifest" | sort)
  actual=$(printf '%s\n' "${files[@]}" | sort)
  if [[ "$listed" != "$actual" ]]; then
    printf 'manifest não lista exatamente os arquivos SQL\n' >&2
    return 1
  fi
  (
    cd "$dir"
    sha256sum --check manifest.sha256 >/dev/null
  )
}

prove_check_fails() {
  local fixture
  fixture=$(mktemp -d)
  if check_dir "$fixture" >/dev/null 2>&1; then
    printf 'checker aceitou diretório sem migration\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"

  fixture=$(mktemp -d)
  printf 'SELECT 1;\n' > "$fixture/nota.sql"
  sha256sum "$fixture/nota.sql" | awk '{print $1 "  nota.sql"}' > "$fixture/manifest.sha256"
  if check_dir "$fixture" >/dev/null 2>&1; then
    printf 'checker aceitou nome de migration inválido\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"

  fixture=$(mktemp -d)
  printf 'SELECT 1;\n' > "$fixture/0001_ok.sql"
  printf '0000000000000000000000000000000000000000000000000000000000000000  0001_ok.sql\n' > "$fixture/manifest.sha256"
  if check_dir "$fixture" >/dev/null 2>&1; then
    printf 'checker aceitou checksum incorreto\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"

  fixture=$(mktemp -d)
  printf 'SELECT 1;\n' > "$fixture/0002_gap.sql"
  (
    cd "$fixture"
    sha256sum 0002_gap.sql > manifest.sha256
  )
  if check_dir "$fixture" >/dev/null 2>&1; then
    printf 'checker aceitou migration fora de sequência\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"
}

psql_exec() {
  PGOPTIONS='-c client_min_messages=warning' psql -v ON_ERROR_STOP=1 --no-psqlrc -q "$@"
}

apply_dir() {
  local dir=$1
  local base version checksum existing count expected
  if [[ "${AUTO_PLATFORM_CI_DATABASE:-}" != "1" ]]; then
    printf 'recuso aplicar migration fora de banco descartável\n' >&2
    return 1
  fi
  if [[ -z "${PGHOST:-}" || -z "${PGUSER:-}" || -z "${PGDATABASE:-}" ]]; then
    printf 'PGHOST, PGUSER e PGDATABASE são obrigatórios\n' >&2
    return 1
  fi
  check_dir "$dir"
  psql_exec --command 'CREATE SCHEMA IF NOT EXISTS platform;'
  psql_exec --command "$(cat <<'SQL'
CREATE TABLE IF NOT EXISTS platform.schema_migrations (
    version text PRIMARY KEY,
    checksum text NOT NULL,
    applied_at timestamptz NOT NULL DEFAULT now()
);
SQL
)"
  apply_once() {
    while IFS= read -r base; do
      [[ -n "$base" ]] || continue
      version=${base%.sql}
      checksum=$(sha256sum "$dir/$base" | awk '{print $1}')
      existing=$(psql_exec -tA --command "SELECT checksum FROM platform.schema_migrations WHERE version = '${version}'")
      if [[ -n "$existing" ]]; then
        if [[ "$existing" != "$checksum" ]]; then
          printf 'migration já aplicada foi editada: %s\n' "$base" >&2
          return 1
        fi
        continue
      fi
      psql_exec <<SQL
BEGIN;
$(cat "$dir/$base")
INSERT INTO platform.schema_migrations (version, checksum)
VALUES ('${version}', '${checksum}');
COMMIT;
SQL
    done < <(sql_files "$dir")
  }
  apply_once
  apply_once
  expected=$(sql_files "$dir" | wc -l | awk '{print $1}')
  count=$(psql_exec -tA --command 'SELECT count(*) FROM platform.schema_migrations;')
  if [[ "$count" != "$expected" ]]; then
    printf 'ledger de migrations divergente: banco=%s arquivos=%s\n' "$count" "$expected" >&2
    return 1
  fi
}

prove_check_fails
check_dir "$migrations"
if [[ "${1:-apply}" == "check" ]]; then
  exit 0
fi
apply_dir "$migrations"
