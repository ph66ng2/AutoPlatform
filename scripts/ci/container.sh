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
integration_api=
postgres_container=
network=
response_file=$(mktemp)
fixture_b=$(mktemp)
cleanup() {
  docker rm --force "$api" "$worker" >/dev/null 2>&1 || true
  if [[ -n "$integration_api" ]]; then
    docker rm --force "$integration_api" >/dev/null 2>&1 || true
  fi
  if [[ -n "$postgres_container" ]]; then
    docker rm --force "$postgres_container" >/dev/null 2>&1 || true
  fi
  if [[ -n "$network" ]]; then
    docker network rm "$network" >/dev/null 2>&1 || true
  fi
  rm -f "$response_file" "$fixture_b"
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

# Exercita a rota publicada dentro da imagem, com banco e identidades fictícias.
network=$(docker network create "ap-ci-integration-$$")
pg_user=ci
pg_password=ci
postgres_container=$(docker run --detach --network "$network" --network-alias ap-ci-postgres --env POSTGRES_USER="$pg_user" --env POSTGRES_PASSWORD="$pg_password" --env POSTGRES_DB=autoplatform postgres:16-alpine)
for _attempt in $(seq 1 40); do
  if docker exec "$postgres_container" pg_isready -h 127.0.0.1 -U "$pg_user" -d autoplatform >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
docker exec "$postgres_container" pg_isready -h 127.0.0.1 -U "$pg_user" -d autoplatform >/dev/null
for migration in "$root"/migrations/[0-9][0-9][0-9][0-9]_*.sql; do
  if [[ "${migration##*/}" == 0007_commercial_fact_keys.sql ]]; then
    continue
  fi
  docker exec -i "$postgres_container" psql -v ON_ERROR_STOP=1 -U "$pg_user" -d autoplatform < "$migration" >/dev/null
done
docker exec -i "$postgres_container" psql -v ON_ERROR_STOP=1 -U "$pg_user" -d autoplatform >/dev/null <<'SQL'
INSERT INTO identity.companies(id,label,status)
VALUES ('00000000-0000-4000-8000-0000000000a1','synthetic-a','active'),
       ('00000000-0000-4000-8000-0000000000d1','synthetic-b','active');
INSERT INTO identity.accounts(id,status)
VALUES ('00000000-0000-4000-8000-0000000000a2','active'),
       ('00000000-0000-4000-8000-0000000000d2','active');
INSERT INTO identity.memberships(company_id,id,account_id,product,role,status)
VALUES ('00000000-0000-4000-8000-0000000000a1',
        '00000000-0000-4000-8000-0000000000a3',
        '00000000-0000-4000-8000-0000000000a2',
        'autoos','operator','active'),
       ('00000000-0000-4000-8000-0000000000d1',
        '00000000-0000-4000-8000-0000000000d3',
        '00000000-0000-4000-8000-0000000000d2',
        'autoos','operator','active');
SQL
database_scheme=postgres
database_url="${database_scheme}://${pg_user}:${pg_password}@ap-ci-postgres:5432/autoplatform"
jwt_secret=synthetic-container-jwt-secret
integration_api=$(docker run --detach --network "$network" --publish "$((port + 2)):8080" --env APP_ENV=staging --env HTTP_BIND=0.0.0.0:8080 --env DATABASE_URL="$database_url" --env JWT_SECRET="$jwt_secret" autoplatform:ci)
wait_http "http://127.0.0.1:$((port + 2))/health"
actual=$(curl --silent --output "$response_file" --write-out '%{http_code}' "http://127.0.0.1:$((port + 2))/ready")
[[ "$actual" == 503 ]]
docker exec -i "$postgres_container" psql -v ON_ERROR_STOP=1 -U "$pg_user" -d autoplatform < "$root/migrations/0007_commercial_fact_keys.sql" >/dev/null
wait_http "http://127.0.0.1:$((port + 2))/ready"
issue_token() {
  AP_CI_JWT_SECRET="$jwt_secret" AP_CI_ACCOUNT_ID="$1" python3 - <<'PY'
import base64
import hashlib
import hmac
import json
import os
import time

def encode(value):
    raw = json.dumps(value, separators=(",", ":")).encode()
    return base64.urlsafe_b64encode(raw).decode().rstrip("=")

message = encode({"alg": "HS256", "typ": "JWT"}) + "." + encode({
    "sub": os.environ["AP_CI_ACCOUNT_ID"],
    "role": "authenticated",
    "aud": "authenticated",
    "exp": int(time.time()) + 600,
})
signature = hmac.new(os.environ["AP_CI_JWT_SECRET"].encode(), message.encode(), hashlib.sha256).digest()
print(message + "." + base64.urlsafe_b64encode(signature).decode().rstrip("="))
PY
}
token=$(issue_token '00000000-0000-4000-8000-0000000000a2')
token_b=$(issue_token '00000000-0000-4000-8000-0000000000d2')
for expected in 201 200; do
  actual=$(curl --silent --show-error --output "$response_file" --write-out '%{http_code}' --header "Authorization: Bearer $token" --header 'x-company-id: 00000000-0000-4000-8000-0000000000a1' --header 'Content-Type: application/json' --data-binary "@$root/contracts/v2/examples/valid/servico.json" "http://127.0.0.1:$((port + 2))/v2/commercial-facts")
  [[ "$actual" == "$expected" ]]
  [[ $(cat "$response_file") == *'"fact_id":"00000000-0000-4000-8000-000000000011"'* ]]
done
# Gate HTTP: o contrato publicado distingue tenants e conserva a chave no replay.
python3 - "$root/contracts/v2/examples/valid/servico.json" "$fixture_b" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as source:
    fact = json.load(source)
fact.update({
    "company_id": "00000000-0000-4000-8000-0000000000d1",
    "fact_id": "00000000-0000-4000-8000-0000000000d5",
    "event_id": "00000000-0000-4000-8000-0000000000d6",
    "os_id": "00000000-0000-4000-8000-0000000000d7",
    "correlation_id": "corr-container-synthetic-b-0001",
    "idempotency_key": "idem-container-synthetic-b-0001",
})
with open(sys.argv[2], "w", encoding="utf-8") as target:
    json.dump(fact, target)
PY
for expected in 201 200; do
  actual=$(curl --silent --show-error --output "$response_file" --write-out '%{http_code}' --header "Authorization: Bearer $token_b" --header 'x-company-id: 00000000-0000-4000-8000-0000000000d1' --header 'Content-Type: application/json' --data-binary "@$fixture_b" "http://127.0.0.1:$((port + 2))/v2/commercial-facts")
  [[ "$actual" == "$expected" ]]
  [[ $(cat "$response_file") == *'"fact_id":"00000000-0000-4000-8000-0000000000d5"'* ]]
done
actual=$(curl --silent --output "$response_file" --write-out '%{http_code}' --header "Authorization: Bearer $token_b" --header 'x-company-id: 00000000-0000-4000-8000-0000000000d1' "http://127.0.0.1:$((port + 2))/v2/commercial-facts/00000000-0000-4000-8000-000000000011")
[[ "$actual" == 404 ]]
actual=$(curl --silent --output "$response_file" --write-out '%{http_code}' --header "Authorization: Bearer $token" --header 'x-company-id: 00000000-0000-4000-8000-0000000000a1' "http://127.0.0.1:$((port + 2))/v2/commercial-facts/00000000-0000-4000-8000-0000000000d5")
[[ "$actual" == 404 ]]
actual=$(curl --silent --show-error --output "$response_file" --write-out '%{http_code}' --header "Authorization: Bearer $token" --header 'x-company-id: 00000000-0000-4000-8000-0000000000a1' 'http://127.0.0.1:'"$((port + 2))"'/v2/commercial-facts?correlation_id=corr-os-synthetic-0001')
[[ "$actual" == 200 ]]
[[ $(cat "$response_file") == *'"fact_id":"00000000-0000-4000-8000-000000000011"'* ]]
snapshot_count=$(docker exec "$postgres_container" psql -At -U "$pg_user" -d autoplatform -c 'SELECT count(*) FROM fiscal.fact_snapshots')
[[ "$snapshot_count" == 2 ]]
expected_tenant=$(printf %s '00000000-0000-4000-8000-0000000000a1' | sha256sum | cut -d' ' -f1)
docker logs "$integration_api" 2>&1 | grep -F "\"tenant\":\"$expected_tenant\"" >/dev/null
docker stop "$postgres_container" >/dev/null
actual=$(curl --silent --output "$response_file" --write-out '%{http_code}' "http://127.0.0.1:$((port + 2))/ready")
[[ "$actual" == 503 ]]
actual=$(curl --silent --output "$response_file" --write-out '%{http_code}' "http://127.0.0.1:$((port + 2))/health")
[[ "$actual" == 200 ]]
actual=$(curl --silent --output "$response_file" --write-out '%{http_code}' --header "Authorization: Bearer $token" --header 'x-company-id: 00000000-0000-4000-8000-0000000000a1' --header 'Content-Type: application/json' --data-binary "@$root/contracts/v2/examples/valid/servico.json" "http://127.0.0.1:$((port + 2))/v2/commercial-facts")
[[ "$actual" == 503 ]]

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
