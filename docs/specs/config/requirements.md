# Config — Requirements

Configuration: credentials, profiles, settings-file discovery

Normative. IDs stable, append-only (CF-R-nnn) — never renumber, never reuse a retired ID. Boundary behavior/stated limitations → [`edge-cases.md`](./edge-cases.md) (CF-E-nnn, numbered independently).

Added via workflow in [`AGENTS.md`](../../../AGENTS.md): gate 1 approves "shall" text before code is written. Deliberately empty until then — an unapproved requirement here would be a spec nobody agreed to.

---

## Sources

**CF-R-001** — The application shall resolve the active repository root as the nearest ancestor of the current working directory (the directory itself included) that contains a `.git` entry.
**CF-R-002** — When no active repository root can be resolved, the application shall print a single-line error to standard error and exit with a non-zero status without entering the alternate screen.
**CF-R-003** — The user-level configuration file shall be `$XDG_CONFIG_HOME/prodgy/config.toml` when `XDG_CONFIG_HOME` is set and non-empty, and `$HOME/.config/prodgy/config.toml` otherwise.
**CF-R-004** — The application shall neither read nor write a repository-level configuration file; an existing `.prodgy.toml` in the repository root is ignored.
**CF-R-006** — The repository settings shall be taken from the user-level `[[repo]]` entry whose `path` equals the canonical path of the active repository root.
**CF-R-007** — When the user-level file yields no entry for the active repository, the application shall start with no repository settings.
**CF-R-008** — A missing user-level file shall be treated as an empty configuration with no profiles and no repository entries.

## Schema

**CF-R-010** — The user-level file shall hold credential profiles as a `credentials` table keyed by profile name, each profile carrying a `kind` of `github`, `jira`, or `bitbucket`.
**CF-R-011** — A `github` profile shall carry `token`.
**CF-R-012** — A `jira` profile shall carry `base_url`, `email`, and `token`.
**CF-R-013** — A `bitbucket` profile shall carry `username` and `app_password`.
**CF-R-014** — The user-level file shall hold repository entries as a `repo` array of tables, each carrying `path`, a `board` section, and a `remote` section.
**CF-R-015** — A `board` section shall carry a `kind` of `github` or `jira`; a `remote` section shall carry a `kind` of `github` or `bitbucket`.
**CF-R-016** — A `github` board section shall carry `owner`, `repo`, and `project`, where `project` is the GitHub Projects number as a positive integer.
**CF-R-017** — A `jira` board section shall carry `project_key`.
**CF-R-018** — A `github` remote section shall carry `owner` and `repo`.
**CF-R-019** — A `bitbucket` remote section shall carry `workspace` and `repo`.
**CF-R-020** — A `board` or `remote` section in the user-level file may carry `credentials`, naming a profile in the same file.
**CF-R-022** — A key not defined by the schema, in any table, shall be a load error naming the key.

## Validation

**CF-R-023** — Two `[[repo]]` entries with the same `path` shall be a load error naming the path.
**CF-R-024** — A `credentials` reference naming a profile absent from the file shall be a load error naming the reference.
**CF-R-025** — A `credentials` reference whose profile `kind` differs from the referencing section's `kind` shall be a load error naming both kinds.
**CF-R-027** — A load error shall be printed as a single line on standard error followed by a non-zero exit, without entering the alternate screen.

## Writing

**CF-R-028** — Writing the active repository's settings to the user-level file shall replace the `[[repo]]` entry whose `path` matches in place, or append a new entry when none matches, and shall leave every other entry unchanged and in its existing order.
**CF-R-029** — Writing a section that carries credential values shall store them in a profile and set the section's `credentials` to that profile's name.
**CF-R-030** — When the section being written already references a profile, that profile shall be updated in place.
**CF-R-031** — When the section being written references no profile, the profile name shall be `board-<dir>` for the board section and `remote-<dir>` for the remote section, where `<dir>` is the file name of the active repository root.
**CF-R-032** — When the derived profile name is already taken, the suffix `-2`, `-3`, … shall be appended, taking the first free name.
**CF-R-034** — Writing shall create the target file's missing parent directories.
**CF-R-036** — When the dialog confirms with both sections sharing the same credential values, one profile shall be stored and both sections shall reference it.
**CF-R-035** — A write failure shall be reported as an error and shall not terminate the application.
