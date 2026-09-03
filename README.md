# Repository Template

A GitHub template repository for projects built with a **spec-driven TDD workflow**: an authoritative specification, gated behavior changes, strict test-first implementation in isolated worktrees, and an independent review before every PR.

Fork it, run one command, and the workflow is set up for your language and your project.

## Use it

```sh
gh repo create my-project --template <you>/repo-template --private --clone
cd my-project
claude
```

Then, in Claude Code:

```
/init
```

If the built-in `/init` answers instead of the bootstrap, use `/init-workspace`.

The bootstrap asks for:

- project name, one-line description, and kind (library / binary / service / CLI / TUI)
- language stack — **Rust**, **Python**, **Node/TypeScript**, **Go**, or **C/C++ with CMake** (Make or Ninja) — detected from a manifest where one exists
- the exact build / test / lint / coverage commands, for confirmation
- capability areas and their requirement-ID prefixes (`FR-R-nnn`, `CL-R-nnn`, …)
- the coverage floor, this project's scope boundaries, and the issue tracker — GitHub (`gh`), Jira (MCP server or REST credentials), local files, or none

and then writes:

| File | What it is |
|---|---|
| `AGENTS.md` | Spec-driven rules, TDD order, build commands, conventions, scope boundaries. The file agents read first. |
| `AGENTS.workflow.md` | The gates, task board, stage-by-stage implementation, review, PR, merge, resume. Orchestrator-only, pulled one heading at a time. |
| `.github/PULL_REQUEST_TEMPLATE.md` | Why / What changed / Approach / Verification — the same four sections the gate 4 PR body uses (GitHub remotes only). |
| `CLAUDE.md` | Thin router into `AGENTS.md`. |
| `.github/copilot-instructions.md` | Same router, for GitHub Copilot. |
| `PRD.md` | Why the project exists — goals, non-goals, users. |
| `ARCHITECTURE.md` | Module map, data flow, concurrency, testing seams. |
| `CONTRIBUTING.md` | The human-facing version of the same rules. |
| `docs/specs/` | The authoritative specification, one directory per area. |
| `.github/workflows/check.yml` or `bitbucket-pipelines.yml` | fmt / lint / types / test / coverage gates — whichever matches the detected remote host. |
| `.lefthook.yml` | Pre-commit checks, plus spec and requirement-ID reminders. |
| `.claude/scripts/extract-section.sh` | Prints one or more markdown sections by heading — reads a slice of a spec file instead of the whole thing. |
| `.claude/scripts/extract-id.sh` | Finds one or more spec entries (`-R-` requirements, `-E-` edge cases) by ID, prints `file:line:text` each — exact spot to edit, no guessing which file it's in. |
| `.claude/scripts/list-sections.sh` | Lists a markdown file's headings verbatim, so an agent knows what `extract-section.sh` can pull without grepping first. |
| `.claude/scripts/token-rank.sh` | Ranks given files by rough token cost of a full Read (`chars/4`), highest first. |
| `.claude/scripts/failed-workflow.sh` | Prints the error output of the most recent failed CI run on a branch — GitHub Actions or Bitbucket Pipelines, auto-detected. |
| `.claude/scripts/issue-view.sh` | Prints an issue's title, body, and comments in compact plain text — Jira or GitHub, auto-detected. |
| `.claude/scripts/pr-view.sh` | Prints a PR's title, body, and comments in compact plain text — GitHub or Bitbucket, auto-detected. Sidesteps a `gh pr view` GraphQL bug (legacy Projects-Classic boards) that errors on the raw command with or without `--comments`. |
| `.claude/scripts/hook-guard-shell.sh` | `PreToolUse` hook, wired in `.claude/settings.json`: denies an unpiped `cat` of a markdown/large file, an unscoped `git show`/`git diff`, an unscoped `find -type f/d`, a raw `gh issue view`, a raw `gh pr view`, a `git commit` while on `main`, or a `git push` targeting `main` — enforces `AGENTS.md`'s shell-output and branch-safety rules instead of just stating them. |
| `.claude/scripts/hook-guard-attribution.sh` | `PreToolUse` hook, wired alongside: denies a `git commit` / `gh pr create|edit|comment` / `gh issue create|comment` whose text or `--body-file` carries a `Co-Authored-By` / "Generated with" trailer. |
| `.claude/scripts/hook-guard-readonly.sh` | `PreToolUse` hook on the `spec-reviewer` agent only (its frontmatter): denies every command that could alter the repo, the worktree, or the filesystem — the reviewer's only sanctioned writes are `review.md` (append) and `review.verdict.md` (rewrite). |
| `.claude/scripts/show-file.sh` | Opens a file for the user in a viewer outside the agent's context (tmux + glow, wslview, tmux + less, else a manual hint); refuses a `*.summary.md` / `*.verdict.md` over 25 lines / 2 KB so agents keep user-facing summaries short. |
| `.claude/scripts/gauntlet.sh` | Runs the full build/test/lint/coverage block with per-step timeouts, logs everything to `artifacts/<slug>/gauntlet.log`, prints one `gauntlet=pass cov=… sha=…` / `gauntlet=fail step=…` line — the only verification output the orchestrator ever holds in context. |

`failed-workflow.sh`, `issue-view.sh`, `pr-view.sh` are bash scripts (`bash .claude/scripts/…`, not `sh` — they use `pipefail`). If a script's output isn't enough (multiple failed jobs, huge issue thread, unexpected API error), never fall back to raw `gh`/`git`/`curl` commands to fill the gap — stop and report the shortfall to the user.

Finally it deletes `templates/` and its own skill, so the fork looks like a normal project.

## The workflow it sets up

`docs/specs/` is normative — code conforms to the spec, not the reverse. Every change to observable behavior passes 4 gates (spec diff → tracking issue → implementation plan → PR), implemented stage by stage under TDD in an isolated git worktree, reviewed by a fresh reviewer after every stage and once more over the whole branch before merge. The orchestrating session never drafts, reads, or pastes content: `spec-author`, `spec-planner`, `spec-implementer` and `spec-reviewer` write files under `.claude/tasks/artifacts/<slug>/` and answer with one status line; the user approves a capped summary opened outside the context. Full gate text: generated `AGENTS.workflow.md` (source: `templates/AGENTS.workflow.md.tmpl`).

State lives on an on-disk task board (`.claude/tasks/`) so an interrupted session resumes instead of restarting.

Running one change through it: `/spec-feature`. Product-owner-only slice — requirement → spec via conversation → tracking issue, stop: `/spec-request`. Independent second-developer review of a finished PR against its ticket, standalone from the implementing session: `/spec-review`.

## What ships

```
.claude/skills/project-init/     the bootstrap (self-deleting)
.claude/skills/init-workspace/   alias, avoids the built-in /init
.claude/skills/spec-feature/     drives one change through the gates
.claude/skills/spec-request/     PO-only slice: requirement -> spec -> ticket, stop
.claude/skills/spec-review/      standalone reviewer: ticket + PR -> review, stop
.claude/skills/context-audit/    finds what's re-read a lot, suggests scripts to cut it
.claude/skills/agent-doc-audit/  checks agent docs for bloat and split-worthy sections
.claude/agents/                  spec-author, spec-planner, spec-implementer, spec-reviewer
.claude/tasks/                   task board, empty until the first run
.claude/settings.json            SessionStart hook: flags an interrupted run; PreToolUse hooks: enforce the shell-output Conventions and the no-attribution-trailer rule
templates/                       every generated file, plus one file per stack
```

## Maintaining this template

Descendant projects run the workflow for real and sometimes improve on it — a sharper agent rule, a fixed script, a Merge-step fix nobody thought of at bootstrap time. `.claude/skills/template-harvest/` (this repo only, self-deleting like `project-init`) points at one or more descendant paths and reports what's worth backporting here, without touching either side — you apply findings by hand.

## Requirements

- [Claude Code](https://claude.com/claude-code)
- `git` 2.5+ (worktrees), `jq` (hooks), `timeout` (coreutils, for `gauntlet.sh`), and `gh` (GitHub) or a Jira MCP server / API token if you want the tracking-issue gate backed by a tracker
- Optionally [lefthook](https://github.com/evilmartians/lefthook) for the pre-commit checks
- Optionally `tmux` + [glow](https://github.com/charmbracelet/glow) (or `wslview` on WSL) so approval files open in a viewer instead of being printed
- Optionally the caveman plugin for compressed agent output — install command offered by the bootstrap itself (`project-init` step 0)
