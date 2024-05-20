_default:
  @just --list --unsorted

# run doc, clippy, and test recipies
all *args:
  just fmt {{args}}
  just doc {{args}}
  just clippy {{args}}
  just test {{args}}

# Format all code
fmt *args:
  cargo fmt --all {{args}}

# run tests on all feature combinations
test *args:
  cargo hack test --feature-powerset {{args}}

# type check and lint code on all feature combinations
clippy *args:
  cargo hack clippy --feature-powerset {{args}} -- -D warnings

# lint documentation on all feature combinations
doc *args:
  RUSTDOCFLAGS='-D warnings' cargo hack doc --feature-powerset {{args}}
