#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

scan() {
  local path=$1
  local status=0
  local rule regex matches file
  while IFS='|' read -r rule regex; do
    [[ -n "$rule" ]] || continue
    matches=$(grep -R -I -l -E -e "$regex" \
      --exclude-dir=.git \
      --exclude-dir=target \
      --exclude-dir=tmp \
      "$path" || true)
    if [[ -n "$matches" ]]; then
      while IFS= read -r file; do
        [[ -n "$file" ]] || continue
        printf 'segredo detectado regra=%s arquivo=%s\n' "$rule" "$file" >&2
        status=1
      done <<<"$matches"
    fi
  done <<'RULES'
private_key|BEGIN [A-Z ]*PRIVATE KEY
aws_access_key|AKIA[0-9A-Z]{16}
github_token|ghp_[A-Za-z0-9]{20,}
slack_token|xox[baprs]-[A-Za-z0-9-]{10,}
postgres_url|postgres(ql)?://[^[:space:]/:]+:[^[:space:]@]+@
RULES
  return "$status"
}

prove_scanner_fails() {
  local fixture
  fixture=$(mktemp -d)
  printf 'token=%s%s\n' 'AKIAIOSFODNN7' 'EXAMPLE' > "$fixture/leak.txt"
  if scan "$fixture" >/dev/null 2>&1; then
    printf 'scanner não reprovou o fixture com credencial\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"
  local clean
  clean=$(mktemp -d)
  printf 'sem credencial\n' > "$clean/ok.txt"
  scan "$clean"
  rm -rf "$clean"
}

prove_scanner_fails
scan "${1:-$root}"
