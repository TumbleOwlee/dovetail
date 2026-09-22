# Config — Edge Cases and Known Limitations

Boundary behavior, error semantics, behavior that is **ugly on purpose**.

Read before "fixing" anything here that looks wrong — an entry is a decision, not an oversight; reversing one is normative → gate 1.

IDs stable, append-only (CF-E-nnn), numbered independently of the area's `-R-` series — never renumber, never reuse a retired ID. Tests cite `-E` IDs exactly like `-R` IDs (`docs/specs/README.md` rules 2 and 8); an entry that only records a limitation is exempt as a class (kind 4 there).

---

## Resolution boundaries

| ID | Condition | Behavior |
|---|---|---|
| **CF-E-001** | `XDG_CONFIG_HOME` is set but empty | The `$HOME/.config` fallback is used; cites CF-R-003 |
| **CF-E-003** | The user-level file exists but fails to parse or validate | Load error; cites CF-R-027 |
| **CF-E-004** | `[[repo]]` `path` matches the active root only after canonicalization (symlinked checkout) | The entry matches; comparison is on canonical paths; cites CF-R-006 |

## Known limitations — intentional constraints

**CF-E-005** — Rewriting the user-level file drops comments and hand formatting: the file is round-tripped through the schema types, not edited in place. Cites CF-R-028.
**CF-E-006** — Only the Linux convention is consulted for the user-level path; Windows and macOS locations are not supported. Cites CF-R-003.
**CF-E-007** — Credential values are stored in plain text in the user-level file; no keyring or encryption. Cites CF-R-010.
**CF-E-008** — Two distinct checkouts of the same remote never share an entry: matching is by path, never by remote URL. Cites CF-R-006.
