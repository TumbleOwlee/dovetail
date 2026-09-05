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

**TU-R-018** — The main view shall consist of a vertical tab line at the left, the active tab's body filling the rest of the rows above the command line, and the command line occupying the bottom line.
**TU-R-019** — The tab line shall stack the tabs `BOARD` and `REPOSITORY` top to bottom, each written one character per row, with one blank column at each side and one blank row above and below its characters, the tabs sharing the line's height evenly; the active tab's cells are drawn in the selected style, the others in the general style.
**TU-R-020** — Ctrl+T followed by `j` shall activate the next tab and Ctrl+T followed by `k` the previous, wrapping at both ends.
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
**TU-R-048** — While the command line is closed and no error is shown, the bottom line shall show the hint `:  command  |  C-t+j C-t+k  tabs`.

## Task Board tab

**TU-R-049** — When the application holds a GitHub board section whose credentials resolve to a stored profile, it shall request the board at start-up, after a confirmed configuration dialog, and on the `reload` command.
**TU-R-050** — While the board request is outstanding, the Task Board tab body shall show a bordered box centered in the body reading `Board is loading..`; when it failed, a bordered box centered in the body in the error color showing the message, hidden as soon as the board is requested again; when no request was possible, the configuration summary as before.
**TU-R-051** — A loaded board shall render its columns side by side in order, each a bordered box spanning the full tab body height, titled with the column name and its card count, the columns sharing the width evenly.
**TU-R-052** — Each card shall render as a bordered box with two columns of horizontal and one row of vertical margin between the border and the content, showing the title in the highlighted text color wrapped over as many lines as the content width requires, followed, only when the issue has labels, by one empty line and one line of label badges.
**TU-R-053** — A label badge shall show the label name on a background of the label's color; an assignee badge shall show `@<login>`.
**TU-R-054** — Exactly one card shall be selected, rendered with a highlighted border; `h` and `l` shall move the selection to the nearest card of the previous or next non-empty column, `j` and `k` to the previous or next card of the same column, all without wrapping.
**TU-R-055** — A column shall scroll so that its selected card is visible.
**TU-R-056** — The `reload` command shall request the board and the pull request list again, each when its request is possible, else show `not configured`.
**TU-R-057** — A card's top border shall carry `#<number>` at its left corner and one assignee badge per assignee at its right corner.
**TU-R-058** — Every view, including the ferrowl-ui widgets, shall paint on the background `#121212`; the board loading box border and text shall use the highlight color.
**TU-R-059** — Enter on the Task Board tab with a selected card shall request that issue's details and open the details overlay, centered, leaving 4 columns free at each side and 3 rows at the top and the bottom, empty but for a bordered box centered in it reading `Loading issue..` until the details arrive, or a bordered box in the error color showing the message when the request failed.
**TU-R-060** — The issue details overlay shall have the layout of TU-R-066 and TU-R-068 with the bar's boxes titled `Assignees`, `Labels`, `Projects`, `Milestones`, `Relationships`, `Development` and `Participants`; a relationship line shows `parent #<number> <title>` or `sub #<number> <title>`; a development line shows `#<number> <title>` of a closing pull request.
**TU-R-061** — Esc or `q` shall close the details overlay; while it is open it shall take every key before the command line and the board.
**TU-R-062** — While the pull request list is outstanding, the Git Remote tab body shall show a bordered box centered in the body reading `Pull requests are loading..`; when it failed, a bordered box centered in the body in the error color showing the message, hidden as soon as the list is requested again; when no request was possible, the configuration summary as before.
**TU-R-063** — A loaded list shall render as a bordered table filling the tab body with the columns `#`, `Title`, `State`, `Author`, `Branch` (`head → base`) and `Updated` (the date), one row per pull request in the order of GH-R-013, the state shown as `open`, `draft`, `merged` or `closed`.
**TU-R-064** — Exactly one row shall be selected and highlighted; `j` and `k` shall move the selection by one row without wrapping.
**TU-R-065** — Enter on the Git Remote tab with a selected row shall request that pull request's details and open the details overlay, centered, leaving 4 columns free at each side and 3 rows at the top and the bottom, empty but for a bordered box centered in it reading `Loading pull request..` until the details arrive, or a bordered box in the error color showing the message when the request failed.
**TU-R-066** — The details overlay shall show a bordered card titled `#<number>` holding the title in the highlighted text color, one line with the state and `by @<author>`, one empty line and the body rendered as markdown (TU-R-077) wrapped to the card width, and below it one bordered box per timeline item in the order loaded, titled `<Type> · @<actor> · <date>` where the type is `Comment`, `Assigned`, `Unassigned`, `Labeled`, `Unlabeled`, `Milestoned`, `Demilestoned`, `Closed`, `Reopened`, `Renamed`, `Merged`, `Review requested`, `Reviewed`, `Referenced` or `Cross-referenced`; a comment box holds the body rendered as markdown (TU-R-077) wrapped to the box width with one row of vertical margin; an event box holds one line describing the change with no vertical margin: the login, the label as a badge, the milestone title, the state reason, `from <previous> to <current>`, the reviewer, the review state followed by the review body rendered as markdown (TU-R-077), `<id> <headline>` or `#<number> <title>` with the repository; `j` and `k` shall scroll the overlay content by one line without leaving its end.
**TU-R-067** — Esc or `q` shall close the details overlay; while it is open it shall take every key before the command line and the table.
**TU-R-068** — The pull request details overlay shall keep a 30 column wide bar at its right holding, top to bottom, bordered boxes titled `Reviewers`, `Assignees`, `Labels`, `Projects`, `Milestone`, `Development` and `Participants`, each listing one entry per line and `None` when empty; a reviewer line shows `@<login>` and the review state as `pending`, `approved`, `changes requested`, `commented` or `dismissed`; a development line shows `#<number> <title>`; each box keeps two columns of horizontal and no vertical margin around its entries, takes the rows its entries need, and the bar is clipped at the overlay's bottom.
**TU-R-069** — One box of the bar shall be focused, its border in the highlight color, the first box by default; Tab shall move the focus to the next box and from the last back to the first, Shift+Tab the reverse; `j` and `k` keep scrolling the left content whatever the focus.
**TU-R-070** — Every color a view uses shall come from one color template in the theme module; a timeline box border shall take the template's color for its type: comment, closed, merged, reopened, labeled (also unlabeled), assigned (also unassigned), milestoned (also demilestoned), renamed, review requested, approved, changes requested, reviewed (any other review state), referenced (also cross-referenced).
**TU-R-071** — In a details overlay the focused `Development` box shall keep a cursor on one of its entries, the first by default, shown with the highlight background; Down and Up shall move it, Tab and Shift+Tab reset it to the first entry; Enter shall close the overlay, switch to the Task Board tab for a pull request's issue entry or to the Git Remote tab for an issue's pull request entry, and open the entry's details overlay as TU-R-059 or TU-R-065 do, the pull request addressed by the repository of its reference.
**TU-R-072** — The pull request details overlay shall show a vertical tab line at the left of its content, laid out as TU-R-019 with the captions `CONVERSATION`, `COMMITS` and `FILES` top to bottom, `CONVERSATION` active when it opens and holding the layout of TU-R-066 and TU-R-068; Ctrl+T followed by `j` (next), `k` (previous, both wrapping) or a digit (TU-R-021) shall switch the overlay's tab; the issue details overlay has no tab line.
**TU-R-073** — The `Commits` tab shall show a table titled `Commits` with the columns `Commit ID`, `Description`, `Author` and `Date`, one row per commit in the order loaded: the abbreviated id, the headline, `@<login>` when the commit is linked to a user else the git author name, and the committed date as `YYYY-MM-DD`; the first row is selected when the overlay opens and the table's keys (`j`, `k`, Down, Up, `g`, `G`) move the selection; the table's border is in the highlight color.
**TU-R-074** — The `Files Changed` tab shall show three bordered panels: a 30 column wide file tree at the left titled `Files`, listing directories as `<name>/` and files by name, each level indented two columns, files colored by status (added in the success color, removed in the error color, others in the text color) and the selected file on the highlight background; and at the right the selected file's old and new state as two read-only code fields, an `Old` side titled by the file's previous path when renamed else its path, and a `New` side titled by its path, each showing the whole file, every row as a marker column (` `, `-` or `+`) then the line's text, the gutter holding the line's number on that side; the new state is the file's content at the pull request's head commit, the old state that content with the patch applied in reverse; removed lines appear on the old side in the error color, added lines on the new side in the success color; both sides hold the same number of rows, a removed line paired with the added line at the same position in its change, and a blank row without a gutter number on one side where the other holds an unpaired removed or added line.
**TU-R-075** — In the `Files Changed` tab one panel shall be focused, its border in the highlight color, the file tree by default; Tab shall move the focus tree → old → new → tree and Shift+Tab the reverse; `j` and `k` on the tree shall move the file selection, skipping directory lines, and show that file's diff from its top; on a focused diff side the code field's navigation keys, `j` and `k` acting as Down and Up, shall move that side's active line, and the other side shall mirror the active line and scroll offset so the same row faces on both sides; `h` and `l` (also Left and Right) on a focused diff side shall scroll that side one column left or right, up to the last column of its widest row, the other side mirroring the horizontal scroll; vertical moves keep it.
**TU-R-076** — The content of a changed file shall be requested lazily: once for the first tree file when the file list arrives, and once for any other file when it becomes selected, never twice for the same file while the overlay is open; both diff sides read `Loading file..` in the placeholder color while the request runs and the request's error message in the error color when it failed; a removed file requests nothing.
**TU-R-077** — A markdown body (an issue or pull request description, a comment, a review body) shall be drawn by the markdown widget in its rendered form: headings without their markers in bold, `•` for list bullets, task boxes, `▎` quote bars, fence bodies in the code style, inline emphasis and links without their markers, every line wrapped at word boundaries to the card width; no line reveals its source, no row is highlighted and no cursor is drawn; the card is as tall as the rendered rows need.
