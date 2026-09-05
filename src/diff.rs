//! A file's old and new state side by side, from its new content and its unified patch.

/// One row of a side: the line's number on that side and its text behind a marker column
/// (` `, `-` or `+`), or a blank filler row facing a line the other side has alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub number: Option<u32>,
    pub text: String,
}

impl Cell {
    fn line(number: u32, marker: char, text: &str) -> Cell {
        Cell {
            number: Some(number),
            text: format!("{marker}{text}"),
        }
    }

    fn filler() -> Cell {
        Cell {
            number: None,
            text: String::new(),
        }
    }
}

/// The two sides, always of equal length.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Split {
    pub old: Vec<Cell>,
    pub new: Vec<Cell>,
}

/// Lays the whole file out on both sides: unchanged lines come from `new_content`, a hunk's
/// removed lines go to the old side and its added lines to the new side, paired in order with
/// filler rows where a change is uneven. A hunk whose header has no parseable starts is applied
/// where the previous one ended; when `new_content` runs out the patch's own text stands in.
pub fn split_file(new_content: &str, patch: &str) -> Split {
    let lines: Vec<&str> = new_content.lines().collect();
    let mut split = Split::default();
    let mut pos = 0usize;
    let mut old_no = 1u32;
    let mut new_no = 1u32;
    let mut removed: Vec<Cell> = Vec::new();
    let mut added: Vec<Cell> = Vec::new();
    for line in patch.lines() {
        if let Some(header) = line.strip_prefix("@@") {
            flush(&mut split, &mut removed, &mut added);
            if let Some((old, new)) = hunk_starts(header) {
                let target = (new.max(1) - 1) as usize;
                while pos < target && pos < lines.len() {
                    push_context(&mut split, &mut old_no, &mut new_no, lines[pos]);
                    pos += 1;
                }
                old_no = old.max(1);
                new_no = new.max(1);
            }
        } else if let Some(text) = line.strip_prefix('-') {
            removed.push(Cell::line(old_no, '-', text));
            old_no += 1;
        } else if let Some(text) = line.strip_prefix('+') {
            let text = lines.get(pos).copied().unwrap_or(text);
            added.push(Cell::line(new_no, '+', text));
            pos += 1;
            new_no += 1;
        } else if line.starts_with('\\') {
            continue;
        } else {
            flush(&mut split, &mut removed, &mut added);
            let text = lines
                .get(pos)
                .copied()
                .unwrap_or_else(|| line.strip_prefix(' ').unwrap_or(line));
            push_context(&mut split, &mut old_no, &mut new_no, text);
            pos += 1;
        }
    }
    flush(&mut split, &mut removed, &mut added);
    for text in lines.iter().skip(pos) {
        push_context(&mut split, &mut old_no, &mut new_no, text);
    }
    split
}

fn push_context(split: &mut Split, old_no: &mut u32, new_no: &mut u32, text: &str) {
    split.old.push(Cell::line(*old_no, ' ', text));
    split.new.push(Cell::line(*new_no, ' ', text));
    *old_no += 1;
    *new_no += 1;
}

/// The `-<old>[,<len>] +<new>[,<len>]` starts of a hunk header after its `@@`.
fn hunk_starts(header: &str) -> Option<(u32, u32)> {
    let mut parts = header.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let start = |range: &str| range.split(',').next()?.parse().ok();
    Some((start(old)?, start(new)?))
}

/// Pairs the pending removed lines with the pending added ones, in order, filling the shorter side.
fn flush(split: &mut Split, removed: &mut Vec<Cell>, added: &mut Vec<Cell>) {
    let mut left = removed.drain(..);
    let mut right = added.drain(..);
    loop {
        match (left.next(), right.next()) {
            (None, None) => break,
            (left, right) => {
                split.old.push(left.unwrap_or_else(Cell::filler));
                split.new.push(right.unwrap_or_else(Cell::filler));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "@@ -1,4 +1,4 @@\n fn main() {\n-    old();\n-    older();\n+    new();\n }\n+// end\n\\ No newline at end of file\n@@ -10 +10 @@ mod x {\n-a\n+b\n";

    const NEW: &str = "fn main() {\n    new();\n}\n// end\nx\nx\nx\nx\nx\nb\nlast\n";

    fn cell(number: u32, text: &str) -> Cell {
        Cell {
            number: Some(number),
            text: text.into(),
        }
    }

    fn gap() -> Cell {
        Cell::filler()
    }

    #[test]
    /// TU-R-074 — both sides carry the whole file: unchanged lines from the new content on both sides, removed lines on the old side paired with added lines on the new side, a filler row facing an unpaired line, line numbers per side, and the no-newline marker dropped.
    fn ut_split_file_lays_out_whole_file() {
        let split = split_file(NEW, PATCH);
        assert_eq!(split.old.len(), split.new.len());
        let expect_old = vec![
            cell(1, " fn main() {"),
            cell(2, "-    old();"),
            cell(3, "-    older();"),
            cell(4, " }"),
            gap(),
            cell(5, " x"),
            cell(6, " x"),
            cell(7, " x"),
            cell(8, " x"),
            cell(9, " x"),
            cell(10, "-a"),
            cell(11, " last"),
        ];
        let expect_new = vec![
            cell(1, " fn main() {"),
            cell(2, "+    new();"),
            gap(),
            cell(3, " }"),
            cell(4, "+// end"),
            cell(5, " x"),
            cell(6, " x"),
            cell(7, " x"),
            cell(8, " x"),
            cell(9, " x"),
            cell(10, "+b"),
            cell(11, " last"),
        ];
        assert_eq!(split.old, expect_old);
        assert_eq!(split.new, expect_new);
    }

    #[test]
    /// TU-E-037 — a removed file with no new content lists its removed lines on the old side and only filler rows on the new side.
    fn ut_removed_file() {
        let split = split_file("", "@@ -1,2 +0,0 @@\n-one\n-two\n");
        assert_eq!(split.old, vec![cell(1, "-one"), cell(2, "-two")]);
        assert_eq!(split.new, vec![gap(), gap()]);
    }

    #[test]
    /// TU-E-032, TU-E-038 — a malformed hunk header applies where the previous hunk ended; a line with no known prefix is context; when the content runs out the patch's own text stands in; every truncation of a patch and of the content splits without panicking.
    fn ut_malformed_and_truncated_input() {
        let split = split_file("plain\n", "@@ garbage\nplain\n-x\n+y\n");
        assert_eq!(split.old, vec![cell(1, " plain"), cell(2, "-x")]);
        assert_eq!(split.new, vec![cell(1, " plain"), cell(2, "+y")]);
        let split = split_file("", "@@ -1 +1 @@\n context\n+added\n");
        assert_eq!(split.old, vec![cell(1, " context"), gap()]);
        assert_eq!(split.new, vec![cell(1, " context"), cell(2, "+added")]);
        for end in 0..=PATCH.len() {
            if PATCH.is_char_boundary(end) {
                let split = split_file(NEW, &PATCH[..end]);
                assert_eq!(split.old.len(), split.new.len());
            }
        }
        for end in 0..=NEW.len() {
            if NEW.is_char_boundary(end) {
                let split = split_file(&NEW[..end], PATCH);
                assert_eq!(split.old.len(), split.new.len());
            }
        }
        assert_eq!(split_file("", ""), Split::default());
        assert_eq!(
            split_file("only\n", ""),
            Split {
                old: vec![cell(1, " only")],
                new: vec![cell(1, " only")],
            }
        );
    }
}
