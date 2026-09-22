# PRD — dovetail

Product framing for `dovetail`. Normative behavior lives in [`docs/specs/`](./docs/specs/); structure lives in [`ARCHITECTURE.md`](./ARCHITECTURE.md). This document states *why* the project exists and what it is and is not for — it does not restate requirements.

## Overview

TUI application to combine full workflow in a single application. Supports Github (Project, Issue, PR), Atlassian (JIRA, Bitbucket) and the Spec Driven Workflow (using local task board). Provides access to all in one place without accessing any of the websites

*(Expand: who runs this, in what setting, against what. Keep it framing, not requirements.)*

## Goals

*(TBD — the properties that decide design trade-offs. Each goal should be able to settle an argument. Examples of the shape:)*

- **Correctness against the authoritative source first.** Behavior follows the published specification, tested against vectors derived from it rather than from our own output.
- **Testable by construction.** External dependencies sit behind a seam, so logic is exercised without the real system.

## Non-goals

*(TBD — what this deliberately will not do. A non-goal here is what lets an agent close a scope question without asking.)*

## Users

*(TBD — who consumes this and how: a library consumer, an operator at a terminal, a service calling an endpoint.)*

## Success criteria

*(TBD — observable conditions under which this is working. Not metrics for their own sake.)*

## Capability areas

The specification is split by area; each owns its behavior end to end.

| Area | Covers | Prefix |
|---|---|---|
| [`tui`](./docs/specs/tui/) | Terminal UI: views, navigation, keybindings, rendering, event loop | `TU-R-*` |
| [`github`](./docs/specs/github/) | GitHub integration: Projects, Issues, Pull Requests via the GitHub API | `GH-R-*` |
| [`atlassian`](./docs/specs/atlassian/) | Atlassian integration: Jira issues and Bitbucket pull requests via REST | `AT-R-*` |
| [`board`](./docs/specs/board/) | Spec-driven workflow: the local task board (`.claude/tasks`) cards and gates | `BD-R-*` |
| [`config`](./docs/specs/config/) | Configuration: credentials, profiles, settings-file discovery | `CF-R-*` |

Cross-cutting concerns live in [`docs/specs/non-functional-requirements.md`](./docs/specs/non-functional-requirements.md).
