@_default:
  just --list

run +ARGS='':
  cargo run -- {{ARGS}}

test +CASES='':
  RUST_BACKTRACE=1 cargo test -- {{CASES}}
