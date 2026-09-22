#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
env_root="$root/infra/environments"
temps=()

cleanup() {
  local status=$?
  rm -rf "${temps[@]}"
  exit "$status"
}
trap cleanup EXIT

expect_lines() {
  local actual=$1
  local expected=$2
  local label=$3
  if [[ "$actual" != "$expected" ]]; then
    printf 'matriz divergente em %s\n' "$label" >&2
    return 1
  fi
}

check_env_dir() {
  local dir=$1
  local matrix="$dir/matrix.json"
  local actual expected fragment env file value key
  local -A distinct_seen=()
  local -A secret_seen=()
  local shared=0
  local nonempty=0
  [[ -f "$matrix" ]] || {
    printf 'matriz de ambientes ausente\n' >&2
    return 1
  }
  actual=$(jq -r 'keys[]' "$matrix")
  expected=$(printf '%s\n' distinct_keys environments forbidden_fragment required_keys secret_keys)
  expect_lines "$actual" "$expected" "chaves" || return 1
  actual=$(jq -r '.environments[]' "$matrix")
  expected=$(printf '%s\n' local staging production)
  expect_lines "$actual" "$expected" "ambientes" || return 1
  actual=$(jq -r '.required_keys[]' "$matrix")
  expected=$(printf '%s\n' \
    APP_ENV HTTP_BIND RUST_LOG RUN_MIGRATIONS_ON_STARTUP \
    DATABASE_URL SUPABASE_URL SUPABASE_ANON_KEY SUPABASE_SERVICE_ROLE_KEY \
    SUPABASE_PROJECT_REF BACKUP_TARGET LOG_SINK ALERT_CHANNEL)
  expect_lines "$actual" "$expected" "chaves obrigatórias" || return 1
  actual=$(jq -r '.secret_keys[]' "$matrix")
  expected=$(printf '%s\n' DATABASE_URL SUPABASE_URL SUPABASE_ANON_KEY SUPABASE_SERVICE_ROLE_KEY)
  expect_lines "$actual" "$expected" "segredos" || return 1
  actual=$(jq -r '.distinct_keys[]' "$matrix")
  expected=$(printf '%s\n' SUPABASE_PROJECT_REF BACKUP_TARGET LOG_SINK ALERT_CHANNEL)
  expect_lines "$actual" "$expected" "identificadores" || return 1
  fragment=$(jq -r '.forbidden_fragment' "$matrix")
  [[ "$fragment" == "bmitag" ]] || {
    printf 'fragmento proibido divergente\n' >&2
    return 1
  }
  actual=$(find "$dir" -maxdepth 1 -type f -name '*.env' -printf '%f\n' | sort)
  expected=$(printf '%s\n' local.env production.env staging.env)
  expect_lines "$actual" "$expected" "arquivos" || return 1

  while IFS= read -r env; do
    file="$dir/${env}.env"
    [[ -f "$file" ]] || {
      printf 'arquivo de ambiente ausente\n' >&2
      return 1
    }
    if [[ -n "$(cut -d= -f1 "$file" | sort | uniq -d)" ]]; then
      printf 'chave repetida em %s\n' "$env" >&2
      return 1
    fi
    while IFS= read -r line; do
      [[ "$line" =~ ^[A-Z][A-Z0-9_]*= ]] || {
        printf 'linha inválida em %s\n' "$env" >&2
        return 1
      }
      value=${line#*=}
      if [[ "$value" == *$'\r'* || "$value" == *[[:space:]]* ]]; then
        printf 'valor inválido em %s\n' "$env" >&2
        return 1
      fi
      if [[ "$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')" == *"$fragment"* ]]; then
        printf 'referência ao projeto interno em %s\n' "$env" >&2
        return 1
      fi
    done <"$file"
    actual=$(cut -d= -f1 "$file" | sort)
    expected=$(jq -r '.required_keys[]' "$matrix" | sort)
    expect_lines "$actual" "$expected" "$env" || return 1
    value=$(env_get "$file" APP_ENV) || return 1
    [[ "$value" == "$env" ]] || {
      printf 'APP_ENV divergente em %s\n' "$env" >&2
      return 1
    }
    value=$(env_get "$file" RUN_MIGRATIONS_ON_STARTUP) || return 1
    [[ "$value" == "false" ]] || {
      printf 'migration no startup em %s\n' "$env" >&2
      return 1
    }
    value=$(env_get "$file" HTTP_BIND) || return 1
    [[ -n "$value" ]] || {
      printf 'HTTP_BIND vazio em %s\n' "$env" >&2
      return 1
    }
    value=$(env_get "$file" RUST_LOG) || return 1
    [[ -n "$value" ]] || {
      printf 'RUST_LOG vazio em %s\n' "$env" >&2
      return 1
    }
    while IFS= read -r key; do
      value=$(env_get "$file" "$key") || return 1
      [[ -n "$value" ]] || {
        printf 'identificador vazio em %s\n' "$env" >&2
        return 1
      }
      if [[ -n "${distinct_seen[$value]:-}" ]]; then
        printf 'identificador repetido\n' >&2
        return 1
      fi
      distinct_seen[$value]=1
    done < <(jq -r '.distinct_keys[]' "$matrix")
    while IFS= read -r key; do
      value=$(env_get "$file" "$key") || return 1
      [[ -z "$value" ]] && continue
      nonempty=1
      if [[ -n "${secret_seen[$value]:-}" ]]; then
        shared=1
      fi
      secret_seen[$value]=1
    done < <(jq -r '.secret_keys[]' "$matrix")
  done < <(jq -r '.environments[]' "$matrix")

  if ((shared)); then
    printf 'chave compartilhada entre ambientes\n' >&2
    return 1
  fi
  if ((nonempty)); then
    printf 'segredo versionado na matriz\n' >&2
    return 1
  fi
}

env_get() {
  local file=$1 key=$2 count line
  count=$(grep -c "^${key}=" "$file" || true)
  [[ "$count" == "1" ]] || {
    printf 'chave ausente ou repetida: %s\n' "$key" >&2
    return 1
  }
  line=$(grep "^${key}=" "$file")
  printf '%s' "${line#*=}"
}

fresh_copy() {
  local dest
  dest=$(mktemp -d)
  temps+=("$dest")
  cp -a "$env_root/." "$dest/"
  printf '%s' "$dest"
}

set_value() {
  local file=$1 key=$2 value=$3 tmp
  tmp=$(mktemp)
  temps+=("$tmp")
  awk -v key="$key" -v value="$value" -F= '
    $1 == key { print key "=" value; next }
    { print }
  ' "$file" >"$tmp"
  mv "$tmp" "$file"
}

expect_reject() {
  local dir=$1 label=$2
  if check_env_dir "$dir" >/dev/null 2>&1; then
    printf 'checker aceitou %s\n' "$label" >&2
    exit 1
  fi
}

prove_rejections() {
  local copy staging_ref
  copy=$(fresh_copy)
  if ! check_env_dir "$copy" >/dev/null; then
    printf 'matriz real inválida; falhas controladas inconclusivas\n' >&2
    exit 1
  fi

  copy=$(fresh_copy)
  set_value "$copy/staging.env" SUPABASE_SERVICE_ROLE_KEY "synthetic-shared-key"
  set_value "$copy/production.env" SUPABASE_SERVICE_ROLE_KEY "synthetic-shared-key"
  expect_reject "$copy" "chave compartilhada"

  copy=$(fresh_copy)
  set_value "$copy/local.env" DATABASE_URL "synthetic-unique-key"
  expect_reject "$copy" "segredo versionado"

  copy=$(fresh_copy)
  staging_ref=$(env_get "$copy/staging.env" SUPABASE_PROJECT_REF)
  set_value "$copy/production.env" SUPABASE_PROJECT_REF "$staging_ref"
  expect_reject "$copy" "identificador repetido"

  copy=$(fresh_copy)
  set_value "$copy/production.env" RUN_MIGRATIONS_ON_STARTUP "true"
  expect_reject "$copy" "migration no startup"

  copy=$(fresh_copy)
  set_value "$copy/local.env" BACKUP_TARGET "prefix-${fragment_token}-suffix"
  expect_reject "$copy" "projeto interno"

  copy=$(fresh_copy)
  set_value "$copy/staging.env" LOG_SINK "$(printf '%s' "$fragment_token" | tr '[:lower:]' '[:upper:]')"
  expect_reject "$copy" "projeto interno em maiúsculas"

  copy=$(fresh_copy)
  grep -v '^ALERT_CHANNEL=' "$copy/local.env" >"$copy/local.env.cut"
  mv "$copy/local.env.cut" "$copy/local.env"
  expect_reject "$copy" "chave ausente"

  copy=$(fresh_copy)
  set_value "$copy/staging.env" APP_ENV "production"
  expect_reject "$copy" "APP_ENV divergente"

  copy=$(fresh_copy)
  set_value "$copy/production.env" LOG_SINK ""
  expect_reject "$copy" "identificador vazio"

  copy=$(fresh_copy)
  jq '.forbidden_fragment = "zzzz"' "$copy/matrix.json" >"$copy/matrix.json.next"
  mv "$copy/matrix.json.next" "$copy/matrix.json"
  expect_reject "$copy" "fragmento proibido enfraquecido"
}

fragment_token=$(printf '%s%s' 'bmi' 'tag')
prove_rejections
check_env_dir "$env_root"

example_values=$(grep -E '^[A-Z][A-Z0-9_]*=' "$root/.env.example" | sort)
local_values=$(sort "$env_root/local.env")
if [[ "$example_values" != "$local_values" ]]; then
  printf '.env.example divergiu de local.env\n' >&2
  exit 1
fi

scan_dir=$(mktemp -d)
temps+=("$scan_dir")
cp -a "$env_root/." "$scan_dir/"
cp "$root/.env.example" "$scan_dir/dotenv.example"
bash "$root/scripts/ci/secrets.sh" "$scan_dir" >/dev/null
