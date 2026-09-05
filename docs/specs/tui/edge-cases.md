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
| **TU-E-022** | The remote section is Bitbucket, or credentials are missing | The Git Remote tab keeps showing the configuration summary; cites TU-R-062 |
| **TU-E-023** | The repository has no pull requests | The table shows only its header; cites TU-R-063 |
| **TU-E-024** | A pull request has no comments | The comments card reads `No comments`; cites TU-R-066 |
| **TU-E-025** | Pull request details arrive after the overlay was closed | Discarded; cites TU-R-065 |
| **TU-E-026** | The right bar's boxes need more rows than the overlay has | Later boxes are cut off at the bottom; the left content still scrolls; cites TU-R-068 |
| **TU-E-027** | An issue or pull request has no timeline items | One box titled `Timeline` reading `No activity` follows the description card; cites TU-R-066 |
| **TU-E-028** | A timeline item's actor account was deleted | The title shows `@ghost`; cites TU-R-066 |
| **TU-E-029** | Enter in a details overlay with the focus outside `Development`, or on an empty `Development` box | Nothing happens; the overlay stays open; cites TU-R-071 |
| **TU-E-030** | Enter on a `Development` entry while the target tab's list is unavailable, loading or failed | The tab switches and the overlay opens anyway; the list underneath keeps its state; cites TU-R-071 |
| **TU-E-031** | The selected changed file has no patch | Both diff sides read `No diff available` in the placeholder color; no content is requested; cites TU-R-074, TU-R-076 |
| **TU-E-032** | A patch holds a hunk header without parseable line starts, or a line with no `+`, `-`, space or `\\` prefix | The hunk is applied where the previous one ended; the line is shown as context; a truncated patch never fails to render; cites TU-R-074 |
| **TU-E-033** | The pull request has no changed files | The tree reads `None`; both diff sides are empty; `j`, `k` and Tab do nothing harmful; cites TU-R-074 |
| **TU-E-034** | Ctrl+T in the issue details overlay, or a digit beyond the last overlay tab after Ctrl+T | Consumed without effect; cites TU-R-072 |
| **TU-E-035** | The selected file is binary at the head commit | Both diff sides read `Binary file` in the placeholder color; the file stays in the tree; cites TU-R-074 |
| **TU-E-036** | The selected file's content is too large for the API to return as text | Both diff sides read `File too large` in the placeholder color; the file stays in the tree; cites TU-R-074 |
| **TU-E-037** | The selected file was removed | The old side lists the patch's removed lines, the new side only blank rows; cites TU-R-074, TU-R-076 |
| **TU-E-038** | The head content holds fewer lines than the patch addresses | The patch's own text stands in for the missing lines; never a crash; cites TU-R-074 |
| **TU-E-039** | A file's content arrives after the overlay was closed, or for a path not in the tree | Discarded; cites TU-R-076 |
| **TU-E-040** | Focus moves to a diff side while it shows a notice instead of a file | The side's border still shows the focus; navigation keys do nothing; cites TU-R-075 |
| **TU-E-041** | `h` at the first column, or `l` at the end of the line, on a diff side | Nothing moves; the column never wraps to another line; cites TU-R-075 |
| **TU-E-042** | The active line of a diff side moves onto a shorter line than the current column | The column clamps to that line's end and the horizontal scroll follows; cites TU-R-075 |
| **TU-E-043** | A markdown body is empty or whitespace only | The card holds only its header lines; the body takes no rows; cites TU-R-077 |
