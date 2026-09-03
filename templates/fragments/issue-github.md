Fills `{{ISSUE_WORKFLOW}}` when project tracks issues on GitHub. Copy body below (under the rule) into AGENTS.workflow.md's gate 1b section.

---

- Candidates: `gh issue list --state all --search "<goal keywords>"` — numbers only, the orchestrator never reads a body; `spec-author` reads any candidate with `bash .claude/scripts/issue-view.sh <number>`, never raw `gh issue view`. Reuse + reference its number; never open a second.
- File from the approved file, never retyped: `gh issue create --title "$(head -1 .claude/tasks/artifacts/<slug>/issue.md)" --body-file <(tail -n +2 .claude/tasks/artifacts/<slug>/issue.md)`.
- Later amendment: `gh issue comment <number> --body-file .claude/tasks/artifacts/<slug>/issue-comment.md`. Never edit the body.
