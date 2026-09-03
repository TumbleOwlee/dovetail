# CLAUDE.md

> Uninitialized template. No product code, no real project described here.

## Asked to init/set up this repo?

Do not write a codebase-summary CLAUDE.md — no codebase exists. Read
[`.claude/skills/project-init/SKILL.md`](./.claude/skills/project-init/SKILL.md)
and follow it end to end (also `/init-workspace`, `/project-init`).

See its frontmatter `description` for what it asks and generates.

## What this is

Spec-driven TDD: `docs/specs/` authoritative. Gates: spec diff → tracking
issue → implementation plan → implementation → independent review → PR.
Implementation = strict TDD, isolated git worktree. No agent self-report
counts as verification.

| Path | Purpose |
|---|---|
| `.claude/skills/project-init/` | Bootstrap. Deleted after run. |
| `.claude/skills/template-harvest/` | Audits a template descendant for improvements to backport here. This repo only — deleted after bootstrap, same as `project-init`. |
| `.claude/skills/spec-feature/` | Drives one behavior change through gates. Kept. |
| `.claude/skills/spec-scaffold-init/` | Adds spec dir + routing pointer only, no full bootstrap. Kept. |
| `.claude/skills/spec-coverage-audit/` | Finds code with no requirement, proposes approved diff. Kept. |
| `.claude/agents/` | `spec-author`, `spec-planner`, `spec-implementer`, `spec-reviewer`. Kept. |
| `templates/` | Source for generated files. Deleted after bootstrap. |

Post-init: this file becomes a thin router into `AGENTS.md`.

## RTK (Rust Token Killer) — Token-Optimized Commands

@./templates/fragments/rtk-instructions.md
