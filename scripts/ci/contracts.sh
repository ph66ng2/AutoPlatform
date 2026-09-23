#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
dir="$root/contracts/v1"

expect_fail() {
  local label=$1
  shift
  if "$@" >/dev/null 2>&1; then
    printf 'checker aceitou %s\n' "$label" >&2
    exit 1
  fi
}

check_manifest() {
  local target=$1 listed expected
  (
    cd "$target"
    sha256sum --check --strict manifest.sha256 >/dev/null
  ) || return 1
  listed=$(awk '{print $2}' "$target/manifest.sha256")
  expected=$(printf '%s\n' \
    entitlement.schema.json \
    estado-financeiro.schema.json \
    estoque.schema.json \
    fato-comercial.schema.json \
    fiscal-document.schema.json \
    payment-intent.schema.json)
  if [[ "$listed" != "$expected" ]]; then
    printf 'manifest não lista os schemas na ordem publicada\n' >&2
    return 1
  fi
}

prove_rejections() {
  local copy
  copy=$(mktemp -d)
  cp -a "$dir/." "$copy/"
  if ! check_manifest "$copy"; then
    printf 'manifest real inválido; falhas controladas inconclusivas\n' >&2
    rm -rf "$copy"
    exit 1
  fi
  local first rest
  first=$(head -n1 "$dir/manifest.sha256")
  rest=$(tail -n +2 "$dir/manifest.sha256")
  printf '0000000000000000000000000000000000000000000000000000000000000000  %s\n%s\n' "${first#*  }" "$rest" >"$copy/manifest.sha256"
  expect_fail "hash divergente" check_manifest "$copy"
  rm -rf "$copy"
}

prove_rejections
check_manifest "$dir"
diff -q "$dir/manifest.sha256" "$root/examples/autoos-consumer/expected.sha256" >/dev/null
diff -q "$dir/manifest.sha256" "$root/examples/autobo-consumer/expected.sha256" >/dev/null
bash "$root/examples/autoos-consumer/verify.sh"
bash "$root/examples/autobo-consumer/verify.sh"
