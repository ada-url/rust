#!/bin/bash
set -o errexit -o nounset -o pipefail
cd "$(dirname "$0")"

msg() {
  echo "$@" >&2
}

extra_args=()
if [[ -n "${SKIP_FEATURES:-}" ]]; then
  extra_args+=(--skip "$SKIP_FEATURES")
fi

msg 'STEP: Test'
cargo hack test --feature-powerset "${extra_args[@]}"

msg 'STEP: Clippy'
cargo hack clippy --feature-powerset "${extra_args[@]}" -- -D warnings

msg 'STEP: Doc'
RUSTDOCFLAGS='-D warnings' cargo hack doc "${extra_args[@]}" --feature-powerset
