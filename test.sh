#!/bin/bash
set -o errexit -o nounset -o pipefail
cd "$(dirname "$0")"

msg() {
  echo "$@" >&2
}

cmd() {
  echo "Running $*" >&2
  "$@"
}

msg 'TEST: no features'
cmd cargo test --no-default-features
cmd cargo clippy --no-default-features
cmd env RUSTDOCFLAGS='-D warnings' cargo doc --no-default-features

msg 'TEST: std'
cmd cargo test --no-default-features --features=std
cmd cargo clippy --no-default-features --features=std
cmd env RUSTDOCFLAGS='-D warnings' cargo doc --no-default-features --features=std

msg 'TEST: serde'
cmd cargo test --no-default-features --features=serde
cmd cargo clippy --no-default-features --features=serde
cmd env RUSTDOCFLAGS='-D warnings' cargo doc --no-default-features --features=serde

msg 'TEST: std, serde'
cmd cargo test --no-default-features --features=std,serde
cmd cargo clippy --no-default-features --features=std,serde
cmd env RUSTDOCFLAGS='-D warnings' cargo doc --no-default-features --features=std,serde
