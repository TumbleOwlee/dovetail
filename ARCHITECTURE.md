# Architecture — prodgy

Module map, data flow, concurrency model of `prodgy`. What: [`docs/specs/`](./docs/specs/). Why: [`PRD.md`](./PRD.md).

Structure only — no normative "shall" statements (those belong to specs only). Changes with refactors.

## Layout

TUI application, stack: Rust (stable toolchain, pinned via `rust-toolchain.toml`).

| Module | Responsibility | Spec area |
|---|---|---|
| `main` | CLI, repository root discovery, loading both files, runtime and terminal setup, panic hook | `config`, `tui` |
| `config` | Serde schema of both files, resolution, validation, profile derivation, writers, origin URL parsing | `config` |
| `app` | Held state: configuration, settings, active tab, open dialog, command line; key dispatch; frame layout | `tui` |
| `event` | The async loop over terminal events and application messages; runs fetches the app queues and sends their outcomes back | `tui` |
| `github` | GitHub API calls via GraphQL: listing an owner's Projects (v2), loading a project board's Status columns and issue cards, loading one issue's details by node id, listing a repository's pull requests | `github` |
| `atlassian` | Atlassian REST calls: listing a Jira site's projects | `atlassian` |
| `command` | `Cmd` parser and the help rows | `tui` |
| `view::tabs` | Tab line and per-tab summary body | `tui` |
| `view::remote` | Git Remote tab body: the pull request table with one selected row | `tui` |
| `view::loading` | Centered box for an outstanding request | `tui` |
| `view::board` | Task Board tab body: columns of issue cards with labels and assignee badges, one selection, keyboard navigation | `tui` |
| `view::command_line` | The `:` prompt, the help box above it while open, and the hint bar, error or notice shown while closed | `tui` |
| `view::dialog::config` | The configuration dialog: kind selections, per-kind fields, validation, forms | `tui` |
| `view::dialog::issue` | The issue details overlay: header, badges, scrolled body, loading and failure states | `tui` |

`board` has no code yet.

## Data flow

A key event is read by a blocking thread in `event`, sent over a channel, and handed to `app`, which offers it to the open configuration dialog, then the open issue overlay, then the open command line, then the main view. A submitted command line goes through `command::parse` back into `app`. Writes go from `app` through `config::store` to disk; a confirmed dialog derives profiles through `config::profile` first. The `config` command on a repository with stored credentials builds the dialog, holds it as waiting, and queues a `FetchRequest`; the loop spawns it with a shared `reqwest::Client` and the outcome returns as an `event::Message` that either opens the waiting dialog with its list or reports the failure on the command line. The board follows the same path: `app` queues a `FetchRequest::Board` at start-up, after a confirmed dialog and on `:reload`, and the returned `Message::Board` becomes the `BoardState` the Task Board tab renders. Every frame is rendered from `app` state alone.

## Concurrency model

One tokio runtime. The loop task owns `App` and the terminal. A std thread reads terminal events and feeds a channel; a second channel carries `event::Message` values from the spawned fetch tasks. No shared mutable state.

## Error handling

`config::ConfigError` is the only error type. Before the alternate screen it is printed and the process exits non-zero; afterwards it is shown in the dialog or the command line and the application keeps running.

## Testing seams

Rendering targets ratatui's `TestBackend` through ferrowl-ui's `DrawSurface`. The loop takes its event channel as a parameter, so tests feed keys without a terminal. File paths are parameters everywhere, so tests use a scratch directory. The origin URL parser is pure; only the `git config` call around it is untested.
