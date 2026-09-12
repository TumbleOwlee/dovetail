# Atlassian — Requirements

Atlassian integration: Jira issues and Bitbucket pull requests via REST

Normative. IDs stable, append-only (AT-R-nnn) — never renumber, never reuse a retired ID. Boundary behavior/stated limitations → [`edge-cases.md`](./edge-cases.md) (AT-E-nnn, numbered independently).

Added via workflow in [`AGENTS.md`](../../../AGENTS.md): gate 1 approves "shall" text before code is written. Deliberately empty until then — an unapproved requirement here would be a spec nobody agreed to.

---

<!--
Shape of an entry — delete this comment when the first requirement lands.
One physical line per statement, however long — never wrapped (see
docs/specs/README.md rule 6; keeps `grep -rn <ID> docs/specs/` exact-match).
One rule per entry (rule 9). Headings unnumbered — extract-section.sh matches
heading text verbatim.

## <Group name>

**AT-R-001** — The <subject> shall <observable outcome> when <condition>.
-->

## Jira projects

**AT-R-001** — The application shall list a Jira site's projects through `GET <base_url>/rest/api/3/project/search`, authenticating with HTTP basic auth of the profile's email and token, and read `key` and `name` from each `values` entry.
**AT-R-002** — A non-success HTTP status or an undecodable body shall be a typed error carrying the status or the decode message.
**AT-R-003** — Listing shall request at most 100 projects; further pages are not fetched.
