# Contributing

## Principles

- Keep platform boundaries explicit.
- Prefer typed contracts over implicit JSON shapes.
- Document platform limitations honestly.
- Favor small, reviewable changes with tests.

## Development Setup

1. Install Rust stable, Node.js 20+, and `pnpm`.
2. Run `cargo test --workspace`.
3. Run `pnpm install`.
4. Run `pnpm check`.

## Pull Requests

- Include tests for logic changes.
- Update docs when behavior or public interfaces change.
- Avoid broad refactors mixed with feature work.
- Keep Linux X11 and Wayland behavior explicitly separated.

## Commit Hygiene

- Use clear, focused commits.
- Reference issues where relevant.
- Keep CI green before requesting review.
