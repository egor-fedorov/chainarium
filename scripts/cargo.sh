#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Optional cloud-local installation; normal rustup installations also work.
if ! command -v cargo >/dev/null && [[ -x /workspace/.chainarium-tools/cargo/bin/cargo ]]; then
  export CARGO_HOME=/workspace/.chainarium-tools/cargo
  export RUSTUP_HOME=/workspace/.chainarium-tools/rustup
  export PATH="$CARGO_HOME/bin:$PATH"
fi
exec cargo "$@"
