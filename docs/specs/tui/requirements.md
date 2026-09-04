# TUI — Requirements

Terminal UI: views, navigation, keybindings, rendering, event loop

Normative. IDs stable, append-only (TU-R-nnn) — never renumber, never reuse a retired ID. Boundary behavior/stated limitations → [`edge-cases.md`](./edge-cases.md) (TU-E-nnn, numbered independently).

Added via workflow in [`AGENTS.md`](../../../AGENTS.md): gate 1 approves "shall" text before code is written. Deliberately empty until then — an unapproved requirement here would be a spec nobody agreed to.

---

## Terminal

**TU-R-001** — The application shall run on the terminal's alternate screen in raw mode and restore the original screen and mode on exit.
**TU-R-002** — A panic shall restore the terminal before the panic message is printed.
**TU-R-003** — Ctrl+C shall be consumed without effect; the application quits only through the `q` command or the first-run dialog's Esc.
**TU-R-004** — A key event shall be offered to the configuration dialog when it is open, else to the command line when it is open, else to the main view; a consumed event shall not reach the next handler.

## Configuration dialog

**TU-R-005** — When the application starts with no repository settings, the configuration dialog shall be open over the main view.
**TU-R-006** — The `config` command shall open the configuration dialog with the current repository settings loaded into its selections and fields as editable values.
**TU-R-007** — The dialog shall show a Task Board section with a kind selection offering GitHub and Jira, and a Git Remote section with a kind selection offering GitHub and Bitbucket.
**TU-R-008** — Each section shall show exactly the input fields of its selected kind: board GitHub shows owner, repo, project, token; board Jira shows base URL, email, token, project key; remote GitHub shows owner, repo, token; remote Bitbucket shows workspace, repo, username, app password.
**TU-R-009** — An empty owner, repo, workspace, or repo-slug field shall show as placeholder the value derived from the repository's `origin` remote URL when that URL is a GitHub or Bitbucket URL of the section's kind.
**TU-R-010** — Ctrl+F in an empty field with a placeholder shall replace the field's content with the placeholder.
**TU-R-011** — Tab shall move focus to the next visible widget and Shift+Tab to the previous, wrapping at both ends, in the order board kind, board fields, remote kind, remote fields.
**TU-R-012** — Enter shall confirm the dialog when every visible input field is non-empty.
**TU-R-013** — Enter with at least one empty visible input field shall not confirm and shall move focus to the first empty visible field in display order.
**TU-R-014** — Confirming shall write the settings to the user-level configuration file, apply them to the running application, and close the dialog.
**TU-R-015** — Esc shall close the dialog without writing when the application holds repository settings.
**TU-R-016** — Esc shall quit the application when it holds no repository settings.
**TU-R-017** — A dialog write failure shall keep the dialog open and show the error inside it.
**TU-R-037** — A kind selection shall occupy one line inside its border, showing only the selected kind; Up/Down and `k`/`j` cycle through the kinds.

## Main view

**TU-R-018** — The main view shall consist of a tab line at the top, the active tab's body filling the middle, and the command line occupying the bottom line.
**TU-R-019** — The tab line shall list the tabs Task Board and Git Remote in that order, each labelled with its configured kind in brackets, as in `Task Board [Jira]`, and with `[-]` when no settings are held.
**TU-R-020** — Tab shall activate the next tab and Shift+Tab the previous, wrapping at both ends.
**TU-R-021** — The keys `1` and `2` shall activate the Task Board and Git Remote tab respectively.
**TU-R-022** — The Task Board tab body shall show the board section's kind, each identifier key with its value, the referenced profile name or `none`, and `present` or `missing` for credentials.
**TU-R-023** — The Git Remote tab body shall show the remote section's kind, each identifier key with its value, the referenced profile name or `none`, and `present` or `missing` for credentials.
**TU-R-024** — With no repository settings, a tab body shall show a single line stating that the repository is not configured and naming the `config` command.

## Command line

**TU-R-025** — `:` in the main view shall open the command line with an empty input and give it focus.
**TU-R-026** — While the command line is open, it shall offer the command names starting with the typed input as completion suggestions.
**TU-R-027** — Enter in the command line shall execute the trimmed input as a command and close the command line.
**TU-R-028** — Esc in the command line shall close it without executing.
**TU-R-029** — The `q` command shall quit the application.
**TU-R-030** — The `board` command shall activate the Task Board tab.
**TU-R-031** — The `remote` command shall activate the Git Remote tab.
**TU-R-032** — The `w` command shall write the held repository settings to the user-level configuration file.
**TU-R-033** — The `wr` command shall write the held repository settings to the repository-level configuration file without credentials.
**TU-R-034** — An unrecognised command shall show `unknown command: <input>` in the command line area until the next key press.
**TU-R-035** — The `w` and `wr` commands with no repository settings shall show `not configured` in the command line area instead of writing.
**TU-R-036** — A write failure from `w` or `wr` shall show the error in the command line area until the next key press.

## Option lists in the configuration dialog

**TU-R-038** — When the dialog opens and the board section's credentials resolve to a stored GitHub profile, the application shall request that owner's GitHub Projects and, on success, show the project field as a selection of `<number> <title>` entries instead of an input.
**TU-R-039** — When the dialog opens and the board section's credentials resolve to a stored Jira profile, the application shall request that site's projects and, on success, show the project-key field as a selection of `<key> <name>` entries instead of an input.
**TU-R-040** — While a request is outstanding, the field shall stay an input and its title shall end in `(loading…)`.
**TU-R-041** — When a request fails, the field shall stay an input, its title shall end in `(list unavailable)`, and the dialog shall show the error message.
**TU-R-042** — A selection replacing an input shall start on the entry matching the field's current value, or on the first entry when the field is empty; when the field is non-empty and no entry matches, or the list is empty, the field shall stay an input keeping its value with its title ending in `(not listed)`.
**TU-R-043** — Confirming with a selection in place shall use the selected entry's number or key as the field's value.
**TU-R-044** — When the dialog opens without stored settings, the board kind selection shall start on GitHub and the remote kind selection shall start on the `origin` remote's host when that host is GitHub or Bitbucket, GitHub otherwise.
