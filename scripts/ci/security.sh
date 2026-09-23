#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
report="$root/docs/reports/fundacao-sec-gate.md"

prove_report_rejects_go_and_secrets() {
  local fixture
  fixture=$(mktemp -d)
  printf 'Veredito: Go\ntoken=%s%s\n' 'AKIAIOSFODNN7' 'EXAMPLE' >"$fixture/relatorio.md"
  if bash "$root/scripts/ci/secrets.sh" "$fixture" >/dev/null 2>&1; then
    printf 'scanner aceitou relatório com credencial\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  if grep -qi 'veredito: go' "$report"; then
    printf 'relatório oficial declarou Go sozinho\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"
}

prove_report_rejects_go_and_secrets
bash "$root/scripts/ci/secrets.sh" "$root/docs/reports" >/dev/null
bash "$root/scripts/ci/migrations.sh" check
cargo test --manifest-path "$root/Cargo.toml" -p ap-security-tests
