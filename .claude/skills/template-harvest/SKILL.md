---
name: template-harvest
description: Point at one or more projects bootstrapped from this template and find improvements worth backporting — new/fixed .claude/scripts/*.sh, sharper agent or skill wording, workflow-process fixes invented during real use (gates, task board, Merge). Propose only, never edit templates/ or the target project. Use when the user asks to check a downstream project for improvements, harvest changes back, or invokes /template-harvest <path> [<path> ...].
---

# Template harvest

**Concise, compact, facts only.**

This repo only. Never copied into a generated project (see `project-init`'s deletion step) — it has nothing to do once the fork is what it's auditing. Read-only: report, never edit `templates/` or the target project. The user applies findings by hand, same spirit as `agent-doc-audit`.

Direction matters: this skill finds what a project **improved beyond** the template, not what it's **missing from** it. A project behind on a script/wording update needs the normal template→project port (what `project-init` already did once); don't conflate the two. If a diff is pure staleness (project has the old version, template has the current one), note it in one line under "Not harvested" and move on — it's not a finding.

## 1. Inputs

One or more project paths. Confirm each looks like a template descendant: has `.claude/agents/spec-planner.md` and an `AGENTS.md` with a `## Spec-driven` heading (an older descendant may still carry a `.claude/AGENTS.core.md` excerpt — a hand-maintained copy this template no longer generates; its removal in that project is staleness, not a finding). Skip + report any path that doesn't — comparing against an unrelated repo produces noise, not findings.

## 2. Compare, per project

**`.claude/scripts/*.sh`** — diff each against `templates/.claude/scripts/<same name>.sh`. Three outcomes:
- Project has a script the template lacks → candidate addition. Check it's genuinely generic (works on any project's `docs/specs/`/`git`/`gh` shape) before proposing — a script hard-coded to this project's directory layout isn't ready to backport as-is; note what'd need generalizing.
- Same script, content differs → read both; a bug fix, new flag, or batching improvement is a candidate; a project-specific hack (hard-coded path, project-only assumption) is not — say why, don't propose it.
- Template's version is newer/more capable → staleness, not a finding (see §1).

**`.claude/agents/*.md`** (`spec-author.md`, `spec-planner.md`, `spec-implementer.md`, `spec-reviewer.md`) — diff against this repo's own. Ignore reformatting/line-wrap noise. Flag: a new stop-condition, a new "Never" bullet, a clearer rule the project's real usage forced into existence, a batching/wiring pattern this repo hasn't adopted yet. Skip anything that's just this project's stack leaking in (a language-specific example is fine to keep phrased generically if worth harvesting, but don't import the literal stack reference).

**`.claude/skills/*/SKILL.md`** (`spec-feature`, `spec-request`, `spec-review`, `spec-scaffold-init`, `spec-coverage-audit`, `context-audit`, `agent-doc-audit`) — same approach. A project inventing a new skill entirely (not present in this repo) is itself a candidate — evaluate for genericity same as a new script.

**`AGENTS.workflow.md`** (root or `.claude/AGENTS.workflow.md` — location is a layout choice, not a finding; `### Principles`, `### Verify before an approval stop`, `### Task board`, `### Agent hand-off`, every `### Gate`, `### Implement, stage by stage`, `### Reconcile the spec`, `### Merge`, `### Resume an interrupted run`) — should track `templates/AGENTS.workflow.md.tmpl`'s corresponding sections near-verbatim once `{{PLACEHOLDER}}` substitutions are mentally undone. This is the highest-value category: a project running the workflow for real invents fixes here that never make it back without a skill like this one looking. Diff against `templates/AGENTS.workflow.md.tmpl`, flag anything the project added or changed that isn't just its own placeholder value.

**`AGENTS.md` agent-read sections** (`## Spec-driven`, `## TDD — fixed order, every stage`, `## Build / test / lint`, `## Conventions — *`, `## Scope boundaries — ask before`) — build/conventions/scope are inherently project-specific (stack commands, house style); don't diff their content wholesale. Do watch for a new *kind* of rule that generalizes regardless of stack (e.g. a citation convention, a structural constraint on spec files) — those are candidates even inside an otherwise project-specific span.

**`docs/specs/README.md`** vs `templates/docs/specs/README.md.tmpl` — the "rules for writing specs" convention. A project's own evolved rule (stricter ID format, a citation convention) that isn't just area-naming is a candidate.

## 3. Report

One table per project, most-impactful first:

`file — what changed — genuine improvement or project-specific (one line why) — proposed template action (add / reword / skip)`

No praise, no summary paragraph. End with the one-line "Not harvested" list (intro).

## 4. If the user approves a finding

Edit `templates/` (and `.claude/agents/*.md` / `.claude/skills/*/SKILL.md` directly, since those ship as-is, not through template substitution) yourself, generalizing the project-specific wording before landing it — never paste the project's literal text if it names that project's stack, paths, or domain. Then this repo's own `README.md` script/skill table may need the same update `agent-doc-audit`-adjacent changes always do — check it.
