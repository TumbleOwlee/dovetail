# Architecture — prodgy

Module map, data flow, concurrency model of `prodgy`. What: [`docs/specs/`](./docs/specs/). Why: [`PRD.md`](./PRD.md).

Structure only — no normative "shall" statements (those belong to specs only). Changes with refactors.

## Layout

TUI application, stack: Rust (stable toolchain, pinned via `rust-toolchain.toml`).

*(TBD. One row per module/package/crate:)*

| Module | Responsibility |
|---|---|
| *(TBD)* | *(TBD)* |

Map module→spec area where possible; state explicitly where not (else reader assumes the mapping).

## Data flow

*(TBD — path of a request/message/command from entry point to result, naming the module at each hop.)*

## Concurrency model

*(TBD — what runs concurrently, who owns which state, boundaries. "Single-threaded, synchronous" is valid — state it.)*

## Error handling

*(TBD — where errors originate, how they cross module boundaries, public error surface. Typed, never stringly — see AGENTS.md.)*

## Testing seams

*(TBD — where real external world is abstracted for tests: transport trait, clock, filesystem. Test-only seam is still architecture — record it, don't "clean up".)*
