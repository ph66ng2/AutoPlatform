#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

prove_clippy_fails() {
  local fixture
  fixture=$(mktemp -d)
  cargo init --quiet --bin --name gate_clippy "$fixture"
  cat > "$fixture/src/main.rs" <<'EOF'
fn main() {
    let unused_value = 1;
    println!("ok");
}
EOF
  if cargo clippy --manifest-path "$fixture/Cargo.toml" -- -D warnings >/dev/null 2>&1; then
    printf 'clippy não reprovou o fixture com warning\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"
}

prove_clippy_fails
cargo clippy --workspace --all-targets -- -D warnings
