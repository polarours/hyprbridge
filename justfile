set positional-arguments

default:
  @just --list

fmt:
  cargo fmt --all

check:
  cargo check --workspace

test:
  cargo test --workspace

web-install:
  pnpm install

web-check:
  pnpm check
