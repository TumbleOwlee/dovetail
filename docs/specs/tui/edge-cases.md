# TUI — Edge Cases and Known Limitations

Boundary behavior, error semantics, behavior that is **ugly on purpose**.

Read before "fixing" anything here that looks wrong — an entry is a decision, not an oversight; reversing one is normative → gate 1.

IDs stable, append-only (TU-E-nnn), numbered independently of the area's `-R-` series — never renumber, never reuse a retired ID. Tests cite `-E` IDs exactly like `-R` IDs (`docs/specs/README.md` rules 2 and 8); an entry that only records a limitation is exempt as a class (kind 4 there).

---

## Dialog boundaries

| ID | Condition | Behavior |
|---|---|---|
| **TU-E-001** | Kind selection changes after fields of the other kind were filled | Hidden fields keep their values and reappear on switching back; only visible fields are validated and written; cites TU-R-008, TU-R-013 |
| **TU-E-002** | Enter pressed with focus on an input field | Confirms like Enter anywhere else; Enter never inserts a newline; cites TU-R-012 |
| **TU-E-003** | `origin` remote is absent or is neither a GitHub nor a Bitbucket URL | No placeholders are derived; the fields are plainly empty; cites TU-R-009 |

## Command line boundaries

| ID | Condition | Behavior |
|---|---|---|
| **TU-E-004** | Enter with an empty or whitespace-only input | The command line closes and nothing executes; cites TU-R-027 |
| **TU-E-005** | Input with leading or trailing whitespace | Trimmed before matching, so ` q ` quits; cites TU-R-027 |

## Known limitations — intentional constraints

**TU-E-006** — Token and app-password fields render their content in plain text; no masking. Cites TU-R-008.
**TU-E-007** — A terminal smaller than the dialog clips the dialog; it does not scroll. Cites TU-R-007.
**TU-E-008** — Mouse input is not handled anywhere. Cites TU-R-004.
