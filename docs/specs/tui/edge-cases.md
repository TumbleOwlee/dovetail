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

## Option list boundaries

| ID | Condition | Behavior |
|---|---|---|
| **TU-E-009** | The `config` command runs with no stored credentials, first run included | No request is made and the dialog opens at once with plain project inputs; cites TU-R-038, TU-R-039 |
| **TU-E-010** | The kind selection is changed after the list loaded | The loaded selection belongs to the kind it was fetched for and reappears with it; the other kind's field is a plain input; cites TU-R-042 |
| **TU-E-012** | The remote kind changes away from GitHub while the board is GitHub | The remote section's own fields appear with whatever they held before; switching back hides them again; cites TU-R-045, TU-E-001 |
| **TU-E-013** | Stored settings hold differing GitHub owner or repository values in the two sections | The Task Board's values fill the shared fields; confirming writes them to both sections; cites TU-R-046 |
| **TU-E-014** | Ctrl+T followed by a digit with no tab of that index | The prefix is disarmed and the active tab is unchanged; cites TU-R-021, TU-R-047 |
| **TU-E-011** | A response arrives while no dialog is waiting for it | It is discarded; cites TU-R-038 |
| **TU-E-015** | A key is pressed while a request is outstanding | The main view handles it as usual; a second `config` command replaces the waiting dialog and issues a new request; cites TU-R-040 |

## Task Board boundaries

| ID | Condition | Behavior |
|---|---|---|
| **TU-E-016** | The board has no cards | Columns render with their headers and no selection exists; navigation keys do nothing; cites TU-R-054 |
| **TU-E-017** | A single word of a title is wider than the card | It is broken at the card width; cites TU-R-052 |
| **TU-E-018** | Badges are wider than the card | The badge line is truncated; cites TU-R-052 |
| **TU-E-019** | The board section is Jira, or credentials are missing | The Task Board tab keeps showing the configuration summary; cites TU-R-050 |
| **TU-E-020** | Enter on the Task Board tab with no selected card | Ignored; cites TU-R-059 |
| **TU-E-021** | Issue details arrive after the overlay was closed | Discarded; cites TU-R-059 |
