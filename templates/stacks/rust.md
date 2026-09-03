# Stack: Rust

Slot values, Rust ecosystem. Each fenced block = one slot; heading names the placeholder it fills.

**Ask about:** workspace vs single crate (`--workspace` vs `--all-features`); whether `cargo-llvm-cov` is acceptable for coverage.

## `{{STACK_NAME}}`

Rust (stable toolchain, pinned via `rust-toolchain.toml`)

## `{{FULL_COMMANDS}}`

```sh
cargo fmt --check
cargo clippy --all-features --all-targets -- -D warnings
cargo check --all-features
cargo test --all-features
cargo llvm-cov --all-features --fail-under-lines {{COVERAGE_FLOOR}}
```

## `{{NARROW_COMMANDS}}`

```sh
cargo test -p member ut_name      # one unit test (name alone isn't narrow in a workspace — still builds/runs every member's test binary)
cargo test --test integration     # one integration test file
cargo check -p member             # typecheck one workspace member
cargo llvm-cov --all-features --html   # browsable per-line coverage
```

## `{{UNIT_TEST_CONVENTION}}`

`#[cfg(test)] mod tests` at bottom of file under test, functions named `ut_*`.

## `{{INTEGRATION_TEST_CONVENTION}}`

`tests/`, functions named `it_*`.

## `{{ID_CITATION_EXAMPLE}}`

`/// FR-R-012 — …`

## `{{ID_CITATION_BLOCK}}`

```rust
#[test]
/// FR-R-012 — The checksum is computed over the full frame excluding the checksum field.
fn ut_checksum_excludes_trailer() { /* … */ }
```

## `{{SETUP_STEPS}}`

Install toolchain via [rustup.rs](https://rustup.rs/), then:

```sh
cargo build --all-features
```

Coverage gate also needs `cargo-llvm-cov`:

```sh
cargo install cargo-llvm-cov --locked
```

## `{{STACK_CONVENTIONS}}`

- Edition 2024, stable toolchain (`rust-toolchain.toml`); MSRV bump is normative (non-functional requirement).
- No bare `unwrap` outside tests; `expect("why this cannot fail")`.
- Domain values: distinct transparent newtypes wrapped at API entry; mixing two must not compile. Raw integers only for genuinely opaque bytes.
- `#[non_exhaustive]` on public error enums so adding a variant isn't breaking.
- Prefer typed handling over `serde_json::Value` — compiler must catch a wrong field name, not the wire, even if it forces duplication across protocol versions.

## `ci` → `.github/workflows/check.yml`

```yaml
name: check

on:
  push:
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --check

  clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-features --all-targets -- -D warnings

  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --all-features

  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all-features

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@cargo-llvm-cov
      - run: cargo llvm-cov --all-features --fail-under-lines {{COVERAGE_FLOOR}}
```

## `bitbucket-pipelines` → `bitbucket-pipelines.yml`

```yaml
image: rust:latest

pipelines:
  default:
    - step:
        name: fmt
        script:
          - rustup component add rustfmt
          - cargo fmt --check
    - step:
        name: clippy
        script:
          - rustup component add clippy
          - cargo clippy --all-features --all-targets -- -D warnings
    - step:
        name: check
        script:
          - cargo check --all-features
    - step:
        name: test
        script:
          - cargo test --all-features
    - step:
        name: coverage
        script:
          - rustup component add llvm-tools-preview
          - cargo install cargo-llvm-cov --locked
          - cargo llvm-cov --all-features --fail-under-lines {{COVERAGE_FLOOR}}
```

## `lefthook` → `.lefthook.yml`

```yaml
pre-commit:
  piped: true
  commands:

    fmt:
      glob: "*.rs"
      run: cargo fmt -- --check
      fail_text: |
        cargo fmt found formatting issues.
        Run `cargo fmt` to fix them, then re-stage your changes.

    clippy:
      glob: "*.rs"
      run: cargo clippy --all-features --all-targets -- -D warnings
      fail_text: |
        cargo clippy found issues.
        Fix the warnings above, then re-stage your changes.

    # Warning only (never blocks the commit): this project is spec-driven, so a
    # change to src/ usually comes with a docs/specs/ update in the same commit.
    spec-reminder:
      run: |
        staged="$(git diff --cached --name-only)"
        code="$(printf '%s\n' "$staged" | grep -E '^src/' || true)"
        spec="$(printf '%s\n' "$staged" | grep -E '^docs/specs/' || true)"
        if [ -n "$code" ] && [ -z "$spec" ]; then
          echo "note: product code changed but no docs/specs/ file is staged."
          echo "      If this commit changes behavior, update the area's requirements.md too"
          echo "      (see AGENTS.md 'Spec-driven'). This is a reminder, not a failure."
        fi
        exit 0

    # Warning only: every test that pins observable behavior cites its requirement ID
    # in a doc comment directly below the #[test] attribute (see AGENTS.md).
    test-id-reminder:
      glob: "*.rs"
      run: |
        staged="$(git diff --cached --name-only | grep -E '\.rs$' || true)"
        [ -z "$staged" ] && exit 0
        if printf '%s\n' "$staged" | xargs grep -l '#\[\(tokio::\)\?test\]' 2>/dev/null | head -1 > /dev/null; then
          if ! git diff --cached -U1 -- $staged | grep -qE '^\+\s*///\s*({{ID_PREFIX_ALTERNATION}})-R-[0-9]+'; then
            echo "note: tests changed but no requirement ID appears in an added doc comment."
            echo "      Tests pinning observable behavior cite their requirement (see AGENTS.md)."
            echo "      Reminder, not a failure."
          fi
        fi
        exit 0
```

## `config` → `clippy.toml` (default)

```toml
# Deny the lint families that let a panic reach a user of the library.
avoid-breaking-exported-api = false
```

## `config` → `rust-toolchain.toml` (default)

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

## `{{GAUNTLET_STEPS}}` → `.claude/scripts/gauntlet.sh`

Step name, timeout (s), command — one `run` line each. Coverage step present only with a floor.

```sh
run fmt    900  cargo fmt --check
run clippy 900  cargo clippy --all-features --all-targets -- -D warnings
run check  900  cargo check --all-features
run test   1800 cargo test --all-features
run cov    1800 cargo llvm-cov --all-features --fail-under-lines {{COVERAGE_FLOOR}}
```

## `{{COVERAGE_EXTRACT}}`

```sh
cov=$(grep -E '^TOTAL' "$log" | tail -1 | grep -oE '[0-9.]+%' | sed -n 3p)
```
