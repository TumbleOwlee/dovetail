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
**TU-R-019** — The tab line shall list the tabs Task Board and Git Remote in that order, each labelled ` [<index>] <title> [<Kind>] ` with a leading and trailing space and a zero-based index, as in ` [0] Task Board [Jira] `, and with `[-]` when no settings are held.
**TU-R-020** — Ctrl+T followed by `l` shall activate the next tab and Ctrl+T followed by `h` the previous, wrapping at both ends.
**TU-R-021** — Ctrl+T followed by a digit shall activate the tab with that zero-based index.
**TU-R-022** — The Task Board tab body shall show the board section's kind, each identifier key with its value, the referenced profile name or `none`, and `present` or `missing` for credentials.
**TU-R-023** — The Git Remote tab body shall show the remote section's kind, each identifier key with its value, the referenced profile name or `none`, and `present` or `missing` for credentials.
**TU-R-024** — With no repository settings, a tab body shall show a single line stating that the repository is not configured and naming the `config` command.

## Command line

**TU-R-025** — `:` in the main view shall open the command line with an empty input and give it focus.
**TU-R-026** — While the command line is open, a help box listing every command with its description shall be drawn directly above the command line.
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

**TU-R-038** — When the `config` command runs and the board section's credentials resolve to a stored GitHub profile, the application shall request that owner's GitHub Projects and open the dialog only once the list arrived, with the project field a selection of `<title>` entries instead of an input.
**TU-R-039** — When the `config` command runs and the board section's credentials resolve to a stored Jira profile, the application shall request that site's projects and open the dialog only once the list arrived, with the project-key field a selection of `<key> <name>` entries instead of an input.
**TU-R-040** — While a request is outstanding, no dialog shall be open and the command line area shall show `loading projects…`.
**TU-R-041** — When a request fails, the dialog shall not open and the command line area shall show the error until the next key press.
**TU-R-042** — A selection replacing an input shall start on the entry matching the field's current value, or on the first entry when the field is empty; when the field is non-empty and no entry matches, or the list is empty, the field shall stay an input keeping its value with its title ending in `(not listed)`.
**TU-R-043** — Confirming with a selection in place shall use the selected entry's number or key as the field's value.
**TU-R-044** — When the dialog opens without stored settings, the board kind selection shall start on GitHub and the remote kind selection shall start on the `origin` remote's host when that host is GitHub or Bitbucket, GitHub otherwise.
**TU-R-045** — When both sections have kind GitHub, the Git Remote section shall show no owner, repository or token fields, and confirming shall use the Task Board section's owner, repository and token for the remote section.
**TU-R-046** — The owner, repository and GitHub token fields shall each be a single value shared by the Task Board and Git Remote sections: whichever section shows the field edits the same value, and confirming uses it for every GitHub section.
**TU-R-047** — Ctrl+T in the main view shall arm a prefix that consumes exactly the next key; a key that is neither `h`, `l` nor a digit shall disarm it without any other effect.
**TU-R-048** — While the command line is closed and no error is shown, the bottom line shall show the hint `:  command  |  C-t+h C-t+l  tabs`.

## Task Board tab

**TU-R-049** — When the application holds a GitHub board section whose credentials resolve to a stored profile, it shall request the board at start-up, after a confirmed configuration dialog, and on the `reload` command.
**TU-R-050** — While the board request is outstanding, the Task Board tab body shall show `loading board…`; when it failed, the error message; when no request was possible, the configuration summary as before.
**TU-R-051** — A loaded board shall render its columns side by side in order, each headed by the column name and its card count, filling the tab body width evenly.
**TU-R-052** — Each card shall render as a bordered box showing the title on its first line and, on the second, one badge per label and one badge per assignee.
**TU-R-053** — A label badge shall show the label name on a background of the label's color; an assignee badge shall show `@<login>`.
**TU-R-054** — Exactly one card shall be selected, rendered with a highlighted border; `h` and `l` shall move the selection to the nearest card of the previous or next non-empty column, `j` and `k` to the previous or next card of the same column, all without wrapping.
**TU-R-055** — A column shall scroll so that its selected card is visible.
**TU-R-056** — The `reload` command shall request the board again when a request is possible, else show `not configured`.
