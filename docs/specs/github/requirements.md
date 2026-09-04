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
