#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)
diff -q "$here/expected.sha256" "$root/contracts/v1/manifest.sha256" >/dev/null
(
  cd "$root/contracts/v1"
  sha256sum --check --strict "$here/expected.sha256" >/dev/null
)
