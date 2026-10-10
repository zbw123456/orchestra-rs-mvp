#!/usr/bin/env bash
# Source this from repository root. Prefer the user's installed toolchain.
if ! command -v cargo >/dev/null 2>&1; then
  orchestra_toolchain="${ORCHESTRA_TOOLCHAIN_DIR:-$PWD/../../work/toolchain}"
  if [ -x "$orchestra_toolchain/cargo/bin/cargo" ]; then
    export CARGO_HOME="$orchestra_toolchain/cargo"
    export RUSTUP_HOME="$orchestra_toolchain/rustup"
    export PATH="$CARGO_HOME/bin:$PATH"
  fi
fi
export PYTHONPATH="$PWD/scheduler${PYTHONPATH:+:$PYTHONPATH}"
