#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

prove_fmt_fails() {
  local fixture
  fixture=$(mktemp -d)
  cargo init --quiet --bin --name gate_fmt "$fixture"
  printf 'fn main(){println!("x");}\n' > "$fixture/src/main.rs"
  if cargo fmt --manifest-path "$fixture/Cargo.toml" -- --check >/dev/null 2>&1; then
    printf 'fmt não reprovou o fixture mal formatado\n' >&2
    rm -rf "$fixture"
    exit 1
  fi
  rm -rf "$fixture"
}

prove_fmt_fails
cargo fmt --all -- --check
