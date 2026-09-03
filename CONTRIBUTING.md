# Contributing to prodgy

## Setup

Install toolchain via [rustup.rs](https://rustup.rs/), then:

```sh
cargo build --all-features
```

Coverage gate also needs `cargo-llvm-cov`:

```sh
cargo install cargo-llvm-cov --locked
```

Optionally install [lefthook](https://github.com/evilmartians/lefthook) and run `lefthook install` to get the pre-commit checks locally.

## Project layout

See [`ARCHITECTURE.md`](./ARCHITECTURE.md) for the module map and data flow, and [`PRD.md`](./PRD.md) for the product framing.

`prodgy` is **spec-driven**: [`docs/specs/`](./docs/specs/) is the authoritative specification of what it must do, split by capability area. The code is expected to conform to it. Before changing behavior, read the relevant area's `requirements.md` and `edge-cases.md`.

## Test-driven development

Write the test first, watch it fail, then implement. A test written after the code it covers asserts what you built rather than what the specification requires — derive expected values from the authoritative source, not from a debug print of your own implementation.

Every new or changed requirement ships with at least one test whose doc comment cites the requirement ID, directly beside the test declaration:

```rust
#[test]
/// GH-R-012 — A pull request list request that exceeds the configured timeout fails with a typed timeout error.
fn ut_pr_list_timeout_is_typed() { /* … */ }
```

Line coverage must stay at or above **80%**, enforced in CI. Coverage is a floor, not a goal — never pad it with tests that execute code without asserting on it.

## Before submitting

Please make sure the following pass locally:

```sh
cargo fmt --check
cargo clippy --all-features --all-targets -- -D warnings
cargo check --all-features
cargo test --all-features
cargo llvm-cov --all-features --fail-under-lines 80
```

CI runs these on every push **and every pull request**, so anything the pre-commit hook would reject is rejected by CI too.

## Pull requests

- Branch off `main` and open your PR against `main`. Branch naming: `<type>/<slug>` with a conventional-commit type (`feat/`, `fix/`, `docs/`).
- Keep PRs focused — one feature or fix per PR.
- Add or update tests for behavior changes. Unit tests: `#[cfg(test)] mod tests` at bottom of file under test, functions named `ut_*`. Integration tests: `tests/`, functions named `it_*`.
- **Update the spec in the same PR.** When you change behavior, update the relevant `docs/specs/<area>/` file(s) — they are the authoritative source, not a one-time snapshot. New requirements get a fresh, appended ID (never renumber or reuse). A behavior change with no spec change is incomplete.
- PR body follows `.github/PULL_REQUEST_TEMPLATE.md`: **Why**, **What changed** (requirement IDs with their text, or "None — no behavior change."), **Approach**, **Verification** (what you actually ran). Paragraphs unwrapped — GitHub soft-wraps.
- Commit messages: subject ≤ 72 columns, body hard-wrapped at 72 — `git log` never soft-wraps.
- Update the README when you change the public surface.
- PRs are merged to `main` by **squash merge**.
- No tool attribution trailers — no `Co-Authored-By` for an assistant, no "Generated with" line — in commit messages, PR bodies, issues or comments.

Agents working in this repo follow the fuller gated workflow in [`AGENTS.md`](./AGENTS.md); human contributors are welcome to, but the checks above are the hard requirements. `.claude/tasks/` is that workflow's execution state — the directories are tracked, the cards inside are local and gitignored. Nothing there needs maintaining by hand.

## Reporting issues

Open an issue with steps to reproduce, the version (or commit), and your platform.
