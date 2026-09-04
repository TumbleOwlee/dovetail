//! Unified patches as side-by-side rows.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Context,
    Removed,
    Added,
}

/// One side of a row: the line number on that side and the text without its prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    pub number: u32,
    pub text: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A hunk header, verbatim.
    Hunk(String),
    /// A context line on both sides, or a removed line paired with an added one; a side is
    /// `None` when the change has more lines on the other side.
    Lines {
        left: Option<Side>,
        right: Option<Side>,
    },
}

/// The rows of a patch: removed lines of a change pair with its added lines in order.
pub fn split_rows(patch: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut old = 1u32;
    let mut new = 1u32;
    let mut removed: Vec<Side> = Vec::new();
    let mut added: Vec<Side> = Vec::new();
    for line in patch.lines() {
        if let Some(header) = line.strip_prefix("@@") {
            flush(&mut rows, &mut removed, &mut added);
            (old, new) = hunk_starts(header).unwrap_or((1, 1));
            rows.push(Row::Hunk(line.to_string()));
        } else if let Some(text) = line.strip_prefix('-') {
            removed.push(Side {
                number: old,
                text: text.to_string(),
                kind: Kind::Removed,
            });
            old += 1;
        } else if let Some(text) = line.strip_prefix('+') {
            added.push(Side {
                number: new,
                text: text.to_string(),
                kind: Kind::Added,
            });
            new += 1;
        } else if line.starts_with('\\') {
            continue;
        } else {
            flush(&mut rows, &mut removed, &mut added);
            let text = line.strip_prefix(' ').unwrap_or(line);
            let side = |number| {
                Some(Side {
                    number,
                    text: text.to_string(),
                    kind: Kind::Context,
                })
            };
            rows.push(Row::Lines {
                left: side(old),
                right: side(new),
            });
            old += 1;
            new += 1;
        }
    }
    flush(&mut rows, &mut removed, &mut added);
    rows
}

/// The `-<old>[,<len>] +<new>[,<len>]` starts of a hunk header after its `@@`.
fn hunk_starts(header: &str) -> Option<(u32, u32)> {
    let mut parts = header.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let start = |range: &str| range.split(',').next()?.parse().ok();
    Some((start(old)?, start(new)?))
}

/// Pairs the pending removed lines with the pending added ones, in order.
fn flush(rows: &mut Vec<Row>, removed: &mut Vec<Side>, added: &mut Vec<Side>) {
    let mut left = removed.drain(..);
    let mut right = added.drain(..);
    loop {
        match (left.next(), right.next()) {
            (None, None) => break,
            (left, right) => rows.push(Row::Lines { left, right }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "@@ -1,4 +1,5 @@\n fn main() {\n-    old();\n-    older();\n+    new();\n }\n+// end\n\\ No newline at end of file\n@@ -10,2 +11,2 @@ mod x {\n-a\n+b\n";

    fn side(number: u32, text: &str, kind: Kind) -> Option<Side> {
        Some(Side {
            number,
            text: text.into(),
            kind,
        })
    }

    #[test]
    /// TU-R-074 — hunk headers set the line numbers; context lines land on both sides; removed lines pair with added ones in order, the longer side leaving gaps; the no-newline marker is dropped.
    fn ut_split_rows_pairs_changes() {
        let rows = split_rows(PATCH);
        assert_eq!(
            rows,
            vec![
                Row::Hunk("@@ -1,4 +1,5 @@".into()),
                Row::Lines {
                    left: side(1, "fn main() {", Kind::Context),
                    right: side(1, "fn main() {", Kind::Context),
                },
                Row::Lines {
                    left: side(2, "    old();", Kind::Removed),
                    right: side(2, "    new();", Kind::Added),
                },
                Row::Lines {
                    left: side(3, "    older();", Kind::Removed),
                    right: None,
                },
                Row::Lines {
                    left: side(4, "}", Kind::Context),
                    right: side(3, "}", Kind::Context),
                },
                Row::Lines {
                    left: None,
                    right: side(4, "// end", Kind::Added),
                },
                Row::Hunk("@@ -10,2 +11,2 @@ mod x {".into()),
                Row::Lines {
                    left: side(10, "a", Kind::Removed),
                    right: side(11, "b", Kind::Added),
                },
            ]
        );
    }

    #[test]
    /// TU-E-032 — a malformed hunk header is kept as a hunk row and numbering restarts at 1; a line with no known prefix is a context line; every truncation of a patch splits without panicking.
    fn ut_malformed_and_truncated_input() {
        let rows = split_rows("@@ garbage\nplain\n-x\n");
        assert_eq!(
            rows,
            vec![
                Row::Hunk("@@ garbage".into()),
                Row::Lines {
                    left: side(1, "plain", Kind::Context),
                    right: side(1, "plain", Kind::Context),
                },
                Row::Lines {
                    left: side(2, "x", Kind::Removed),
                    right: None,
                },
            ]
        );
        for end in 0..=PATCH.len() {
            if PATCH.is_char_boundary(end) {
                let _ = split_rows(&PATCH[..end]);
            }
        }
        assert!(split_rows("").is_empty());
    }
}
