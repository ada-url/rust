#!/bin/bash
set -o errexit -o nounset -o pipefail
cd "$(dirname "$0")"

msg() {
  echo "$@" >&2
}

msg 'STEP: Test'
cargo hack test --feature-powerset

msg 'STEP: Clippy'
cargo hack clippy --feature-powerset -- -D warnings

msg 'STEP: Doc'
RUSTDOCFLAGS='-D warnings' cargo hack doc --feature-powerset
