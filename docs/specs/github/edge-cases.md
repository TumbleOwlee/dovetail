# GitHub — Edge Cases and Known Limitations

Boundary behavior, error semantics, behavior that is **ugly on purpose**.

Read before "fixing" anything here that looks wrong — an entry is a decision, not an oversight; reversing one is normative → gate 1.

IDs stable, append-only (GH-E-nnn), numbered independently of the area's `-R-` series — never renumber, never reuse a retired ID. Tests cite `-E` IDs exactly like `-R` IDs (`docs/specs/README.md` rules 2 and 8); an entry that only records a limitation is exempt as a class (kind 4 there).

---

<!--
Shape of an entry — delete this comment when the first entry lands.
One physical line per entry, however long — never wrapped (README rule 6),
table rows included. Cite the governing requirement inline.

## <Boundary group>

| ID | Condition | Behavior |
|---|---|---|
| **GH-E-001** | <condition> | <observable behavior>; cites GH-R-0nn |

## Known limitations — intentional constraints

**GH-E-002** — <what is deliberately not done>: <why>. Cites GH-R-0nn.
-->

## Known limitations — intentional constraints

**GH-E-001** — Only the first 100 projects of an owner are listed; pagination is not followed. Cites GH-R-003.

**GH-E-002** — Only 10 labels and 5 assignees per item are loaded; their pagination is not followed. Cites GH-R-006.
**GH-E-003** — A project without a `Status` single-select field yields one `No status` column holding every item. Cites GH-R-005, GH-R-007.
**GH-E-004** — Only 10 labels and 5 assignees of an issue's details are loaded; their pagination is not followed. Cites GH-R-010.
**GH-E-005** — Merged or closed pull requests beyond the 10 most recently updated are not listed. Cites GH-R-012.
**GH-E-006** — Only the first 20 review requests, 20 latest reviews, 10 assignees, 20 labels, 10 projects, 10 closing issues and 20 participants of a pull request are loaded; their pagination is not followed. Cites GH-R-015.
**GH-E-007** — Only the first 10 projects, 20 sub-issues, 10 closing pull requests and 20 participants of an issue's details are loaded; their pagination is not followed. Cites GH-R-010.
**GH-E-008** — A timeline item of a type outside GH-R-017, or one whose actor, assignee, reviewer or source is not a user, team, issue or pull request, is skipped rather than failing the load. Cites GH-R-017.
**GH-E-009** — The issue timeline query spreads no fragment on a pull-request-only item type (merged, review requested, review): GitHub rejects such a fragment inside `IssueTimelineItems` even when the type filter excludes it. Cites GH-R-010.
