Fills `{{ISSUE_WORKFLOW}}` when project has no issue tracker. Copy body below (under the rule) into AGENTS.workflow.md's gate 1b section.

---

- No issue tracker → nothing is filed, but the goal is still written down and approved: `spec-author` drafts `.claude/tasks/artifacts/<slug>/issue.md` exactly as above, the user approves it, it stays in `artifacts/<slug>/`. User approval of `issue.md` is the gate; record `issue: artifacts/<slug>/issue.md` on the parent card.
- Later amendment: append `issue-comment.md` to `issue.md` under a `## Update <date>` heading. Never edit above it.
- At gate 4, `spec-author` carries `issue.md`'s body into `pr.md` as its opening sections, so merged history records what was asked for and what was built.
