# prodgy

TUI application to combine full workflow in a single application. Supports Github (Project, Issue, PR), Atlassian (JIRA, Bitbucket) and the Spec Driven Workflow (using local task board). Provides access to all in one place without accessing any of the websites

> Status: scaffolded, no implementation yet. The first feature goes through
> gate 1 of the workflow in [`AGENTS.md`](./AGENTS.md).

## Install

*(TBD)*

## Usage

*(TBD)*

## Development

```sh
cargo fmt --check
cargo clippy --all-features --all-targets -- -D warnings
cargo check --all-features
cargo test --all-features
cargo llvm-cov --all-features --fail-under-lines 80
```

See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for the workflow and [`AGENTS.md`](./AGENTS.md) for the agent-facing version of it.

## Documentation

| Document | Contains |
|---|---|
| [`PRD.md`](./PRD.md) | Why this exists, goals, non-goals |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | Module map, data flow, concurrency |
| [`docs/specs/`](./docs/specs/) | Authoritative behavior specification |
| [`AGENTS.md`](./AGENTS.md) | Agent workflow and conventions |

## License

*(TBD)*
