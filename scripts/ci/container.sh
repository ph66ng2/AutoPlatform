#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
port=${AP_CI_PORT:-18080}

prove_build_fails() {
  local fixture
  fixture=$(mktemp -d)
  printf 'FROM scratch\nCOPY missing /missing\n' > "$fixture/Dockerfile"
  if docker build --tag autoplatform-ci-negative:bad "$fixture" >/dev/null 2>&1; then
    printf 'build negativo não falhou\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"
}

wait_http() {
  local url=$1
  local _attempt
  for _attempt in $(seq 1 40); do
    if curl --fail --silent "$url" >/dev/null; then
      return 0
    fi
    sleep 0.25
  done
  printf 'timeout ao consultar %s\n' "$url" >&2
  return 1
}

prove_build_fails
docker build --tag autoplatform:ci "$root"

api=$(docker run --detach --publish "${port}:8080" --env APP_ENV=local --env HTTP_BIND=0.0.0.0:8080 autoplatform:ci)
worker=$(docker run --detach --publish "$((port + 1)):8081" --env APP_ENV=local --env HTTP_BIND=0.0.0.0:8081 autoplatform:ci /usr/local/bin/worker)
cleanup() {
  docker rm --force "$api" "$worker" >/dev/null 2>&1 || true
}
trap cleanup EXIT

wait_http "http://127.0.0.1:${port}/health"
wait_http "http://127.0.0.1:$((port + 1))/health"
api_body=$(curl --fail --silent "http://127.0.0.1:${port}/health")
worker_body=$(curl --fail --silent "http://127.0.0.1:$((port + 1))/health")
api_ready=$(curl --fail --silent "http://127.0.0.1:${port}/ready")
worker_ready=$(curl --fail --silent "http://127.0.0.1:$((port + 1))/ready")
[[ "$api_body" == *'"service":"api"'* ]]
[[ "$worker_body" == *'"service":"worker"'* ]]
[[ "$api_ready" == *'"migrations_on_startup":false'* ]]
[[ "$worker_ready" == *'"migrations_on_startup":false'* ]]

prove_image_has_no_secrets() {
  local leak extract
  leak=$(mktemp -d)
  printf '%s%s%s%s\n' 'postgres://' 'user:' 'synthetic' '@db/app' >"$leak/leak.txt"
  if bash "$root/scripts/ci/secrets.sh" "$leak" >/dev/null 2>&1; then
    printf 'scanner aceitou URL de banco com senha\n' >&2
    rm -rf "$leak"
    exit 1
  fi
  rm -rf "$leak"
  extract=$(mktemp -d)
  docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' autoplatform:ci >"$extract/config-env.txt"
  docker history --no-trunc --format '{{.CreatedBy}}' autoplatform:ci >"$extract/history.txt"
  bash "$root/scripts/ci/secrets.sh" "$extract" >/dev/null
  rm -rf "$extract"
}

prove_image_has_no_secrets
