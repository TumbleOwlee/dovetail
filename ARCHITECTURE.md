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
| `event` | The async loop over terminal events and application messages | `tui` |
| `command` | `Cmd` parser and command-name completion | `tui` |
| `view::tabs` | Tab line and per-tab summary body | `tui` |
| `view::command_line` | The `:` prompt with completion and the error slot | `tui` |
| `view::dialog::config` | The configuration dialog: kind selections, per-kind fields, validation, forms | `tui` |

`github`, `atlassian` and `board` have no code yet.

## Data flow

A key event is read by a blocking thread in `event`, sent over a channel, and handed to `app`, which offers it to the open dialog, then the open command line, then the main view. A submitted command line goes through `command::parse` back into `app`. Writes go from `app` through `config::store` to disk; a confirmed dialog derives profiles through `config::profile` first. Every frame is rendered from `app` state alone.

## Concurrency model

One tokio runtime. The loop task owns `App` and the terminal. A std thread reads terminal events and feeds a channel; a second channel carries `event::Message` values from future integration tasks. No shared mutable state.

## Error handling

`config::ConfigError` is the only error type. Before the alternate screen it is printed and the process exits non-zero; afterwards it is shown in the dialog or the command line and the application keeps running.

## Testing seams

Rendering targets ratatui's `TestBackend` through ferrowl-ui's `DrawSurface`. The loop takes its event channel as a parameter, so tests feed keys without a terminal. File paths are parameters everywhere, so tests use a scratch directory. The origin URL parser is pure; only the `git config` call around it is untested.
