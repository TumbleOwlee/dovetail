Fills `{{ISSUE_WORKFLOW}}` when project tracks issues in Jira via an MCP server. Copy body below (under the rule) into AGENTS.workflow.md's gate 1b section.

---

- Search Jira (MCP search tool) for an open issue with the same goal. A candidate's summary and description come back as tool text — hand exactly that text to `spec-author` for the reuse/new decision, nothing more (MCP tools take and return text, not files: the one place issue content passes through the orchestrator's context). Reuse + reference its key; never open a second.
- File via the MCP create-issue tool: `summary` = line 1 of `.claude/tasks/artifacts/<slug>/issue.md` (`sed -n 1p`), `description` = the rest (`tail -n +2`), read only to fill the tool arguments, never retyped.
- Later amendment: the MCP comment tool with `.claude/tasks/artifacts/<slug>/issue-comment.md`'s content the same way. Never edit the description.
- No Jira MCP tool available in this session → stop, report, ask the user to configure one (`claude mcp list` to check) before continuing gate 1b.
