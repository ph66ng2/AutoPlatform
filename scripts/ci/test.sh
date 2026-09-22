#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

prove_test_fails() {
  local fixture
  fixture=$(mktemp -d)
  cargo init --quiet --bin --name gate_test "$fixture"
  cat > "$fixture/src/main.rs" <<'EOF'
fn main() {}

#[test]
fn falha_controlada() {
    panic!("gate");
}
EOF
  if cargo test --manifest-path "$fixture/Cargo.toml" --quiet >/dev/null 2>&1; then
    printf 'cargo test não reprovou o fixture que entra em panic\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"
}

prove_test_fails
cargo test --workspace
