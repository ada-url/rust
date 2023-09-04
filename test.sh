#!/bin/bash
set -o errexit -o nounset -o pipefail
cd "$(dirname "$0")"

msg() {
  echo "$@" >&2
}

skip_features=${SKIP_FEATURES:-}

msg 'STEP: Test'
cargo hack test --feature-powerset --skip="$skip_features"

msg 'STEP: Clippy'
cargo hack clippy --feature-powerset --skip="$skip_features" -- -D warnings

msg 'STEP: Doc'
RUSTDOCFLAGS='-D warnings' cargo hack doc --skip="$skip_features" --feature-powerset
