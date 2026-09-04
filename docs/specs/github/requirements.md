# GitHub — Requirements

GitHub integration: Projects, Issues, Pull Requests via the GitHub API

Normative. IDs stable, append-only (GH-R-nnn) — never renumber, never reuse a retired ID. Boundary behavior/stated limitations → [`edge-cases.md`](./edge-cases.md) (GH-E-nnn, numbered independently).

Added via workflow in [`AGENTS.md`](../../../AGENTS.md): gate 1 approves "shall" text before code is written. Deliberately empty until then — an unapproved requirement here would be a spec nobody agreed to.

---

<!--
Shape of an entry — delete this comment when the first requirement lands.
One physical line per statement, however long — never wrapped (see
docs/specs/README.md rule 6; keeps `grep -rn <ID> docs/specs/` exact-match).
One rule per entry (rule 9). Headings unnumbered — extract-section.sh matches
heading text verbatim.

## <Group name>

**GH-R-001** — The <subject> shall <observable outcome> when <condition>.
-->

## Projects

**GH-R-001** — The application shall list an owner's GitHub Projects (v2) through the GraphQL API, sending the token as a bearer token and querying `repositoryOwner(login)` for `projectsV2` nodes with `number` and `title`.
**GH-R-002** — A non-success HTTP status, a GraphQL `errors` array, or a response missing the owner shall be a typed error carrying the status or the first message.
**GH-R-003** — Listing shall request at most 100 projects; further pages are not fetched.

## Board

**GH-R-004** — The application shall load a project board through the GraphQL API by querying `repositoryOwner(login)`'s `projectV2(number)` for its title, its `Status` single-select field options, and its items, 100 per page, requesting the next page with the previous page's `endCursor` while `hasNextPage` is true and appending each page's cards to the columns of the first.
**GH-R-005** — The board's columns shall be the `Status` field's options in the field's order, followed by a `No status` column.
**GH-R-006** — Each item whose content is an issue shall become a card carrying the issue's node id, title, number, label names with their colors, and assignee logins.
**GH-R-007** — An item shall be placed in the column named by its `Status` value, or in `No status` when it has none or the name matches no option.
**GH-R-008** — An item whose content is absent, a pull request, or a draft issue shall be skipped.
**GH-R-009** — A project that does not exist, or a GraphQL error, shall be a typed error carrying the message.
**GH-R-010** — The application shall load an issue's details through the GraphQL API by querying `node(id)` for its title, number, state, body, author login, URL, repository `nameWithOwner`, label names with their colors, assignee logins, project titles, milestone title, parent issue and sub-issues (number and title), closing pull request references (number, title and repository `nameWithOwner`), participants, and its timeline items 100 per page, requesting the next page with the previous page's `endCursor` while `hasNextPage` is true, filtered to comments and the events of GH-R-017 that exist on issues.
**GH-R-011** — A node that is absent or not an issue, or a GraphQL error, shall be a typed error carrying the message.
**GH-R-012** — The application shall load a repository's pull requests through the GraphQL API by querying `repository(owner, name)`'s `pullRequests` ordered by `UPDATED_AT` descending: every open one, 100 per page, requesting the next page with the previous page's `endCursor` while `hasNextPage` is true, then the 10 most recently updated merged or closed ones in one request, each carrying number, title, state (`OPEN`, `MERGED` or `CLOSED`), draft flag, author login, head and base branch names and `updatedAt`.
**GH-R-013** — The loaded pull requests shall be ordered open first, then merged, then closed, each group most recently updated first.
**GH-R-014** — A repository that does not exist, or a GraphQL error, shall be a typed error carrying the message.
**GH-R-015** — The application shall load a pull request's details through the GraphQL API by querying `repository(owner, name)`'s `pullRequest(number)` for its title, body, state, draft flag, author login and URL, its requested reviewers and latest reviews with their state, assignees, labels with colors, project titles, milestone title, closing issue references (node id, number and title), participants, and its timeline items 100 per page, requesting the next page with the previous page's `endCursor` while `hasNextPage` is true, filtered to comments and the events of GH-R-017.
**GH-R-016** — A pull request that does not exist, or a GraphQL error, shall be a typed error carrying the message.
**GH-R-017** — A timeline item shall carry its actor login, `createdAt` and one of: a comment with its body; assigned or unassigned with the login; labeled or unlabeled with the label name and color; milestoned or demilestoned with the milestone title; closed with the state reason when present; reopened; renamed with the previous and the current title; merged; review requested with the reviewer's login or team name; a review with its state and body; referenced with the commit's abbreviated id and message headline; cross-referenced with the source's number, title and repository `nameWithOwner`.
**GH-R-018** — The pull request details query of GH-R-015 shall also ask for the pull request's commits, 100 per request, each with the commit's abbreviated id, message headline, committed date and author, the author being the GitHub login when the commit is linked to a user and the git author name otherwise.
**GH-R-019** — The application shall load a pull request's changed files through the REST endpoint `GET /repos/{owner}/{repo}/pulls/{number}/files` with `per_page=100`, requesting the following `page` while a page holds 100 entries, each file carrying its path, previous path when renamed, status (added, removed, modified, renamed, copied, changed or unchanged), additions, deletions and its unified patch when present; a non-success status, unreachable host or undecodable body is a typed error.
