Fills `{{ISSUE_WORKFLOW}}` when project tracks issues in Jira via REST API credentials. Copy body below (under the rule) into AGENTS.workflow.md's gate 1b section.

---

- Credentials in `.claude/jira.local.json` (gitignored, never committed, never logged, never quoted in output): `baseUrl`, `email`, `apiToken`. Missing/unreadable → stop, ask the user to run through the bootstrap's Jira setup again.
- Search Jira (REST `/rest/api/3/search`) for an open issue with the same goal; `spec-author` reads any candidate with `bash .claude/scripts/issue-view.sh <key>`, never a raw REST fetch dumped into context. Reuse + reference its key; never open a second.
- File via REST `/rest/api/3/issue`: `summary` = line 1 of `.claude/tasks/artifacts/<slug>/issue.md`, `description` = the rest, both read from the file into the JSON payload (`jq -Rs`), never retyped.
- Later amendment: REST `/rest/api/3/issue/<key>/comment`, body from `.claude/tasks/artifacts/<slug>/issue-comment.md` the same way. Never edit the description.
