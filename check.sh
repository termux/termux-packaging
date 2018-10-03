#!/bin/sh
set -e

cargo fmt --all
touch src/main.rs # https://github.com/rust-lang-nursery/rust-clippy/issues/2604
cargo clippy --all-targets --all-features -- -D warnings
cargo test
