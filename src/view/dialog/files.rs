//! The `Files Changed` tab of the pull request overlay: file tree and split diff.

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::diff::{self, Kind, Row};
use crate::github::files::{ChangedFile, FileStatus};
use crate::view::theme;

const TREE_WIDTH: u16 = 30;

/// The focusable panels, in Tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Tree,
    Old,
    New,
}

/// One line of the file tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub depth: usize,
    /// The directory name with a trailing `/`, or the file name.
    pub name: String,
    /// Index into the files for a file line, `None` for a directory line.
    pub file: Option<usize>,
}

/// The tree of the files sorted by path: a directory line before its entries.
pub fn tree_rows(files: &[ChangedFile]) -> Vec<TreeRow> {
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by(|a, b| files[*a].path.cmp(&files[*b].path));
    let mut rows = Vec::new();
    let mut open: Vec<&str> = Vec::new();
    for index in order {
        let mut parts: Vec<&str> = files[index].path.split('/').collect();
        let name = parts.pop().unwrap_or_default();
        let shared = open.iter().zip(&parts).take_while(|(a, b)| a == b).count();
        open.truncate(shared);
        for dir in &parts[shared..] {
            rows.push(TreeRow {
                depth: open.len(),
                name: format!("{dir}/"),
                file: None,
            });
            open.push(dir);
        }
        rows.push(TreeRow {
            depth: open.len(),
            name: name.to_string(),
            file: Some(index),
        });
    }
    rows
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesState {
    /// Index into the files of the selected one.
    selected: usize,
    focus: Panel,
    scroll: usize,
    /// The split rows of the selected file's patch.
    rows: Vec<Row>,
}

impl FilesState {
    /// The first file selected, the tree focused.
    pub fn new(files: &[ChangedFile]) -> FilesState {
        let selected = tree_rows(files).iter().find_map(|r| r.file).unwrap_or(0);
        FilesState {
            selected,
            focus: Panel::Tree,
            scroll: 0,
            rows: rows_of(files.get(selected)),
        }
    }

    #[cfg(test)]
    pub fn focus(&self) -> Panel {
        self.focus
    }

    #[cfg(test)]
    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn handle_key(&mut self, files: &[ChangedFile], code: KeyCode) {
        match (code, self.focus) {
            (KeyCode::Tab, _) => {
                self.focus = match self.focus {
                    Panel::Tree => Panel::Old,
                    Panel::Old => Panel::New,
                    Panel::New => Panel::Tree,
                }
            }
            (KeyCode::BackTab, _) => {
                self.focus = match self.focus {
                    Panel::Tree => Panel::New,
                    Panel::Old => Panel::Tree,
                    Panel::New => Panel::Old,
                }
            }
            (KeyCode::Char('j' | 'k'), Panel::Tree) => {
                let order: Vec<usize> = tree_rows(files).iter().filter_map(|r| r.file).collect();
                let Some(at) = order.iter().position(|i| *i == self.selected) else {
                    return;
                };
                let at = if code == KeyCode::Char('j') {
                    (at + 1).min(order.len() - 1)
                } else {
                    at.saturating_sub(1)
                };
                if order[at] != self.selected {
                    self.selected = order[at];
                    self.scroll = 0;
                    self.rows = rows_of(files.get(self.selected));
                }
            }
            (KeyCode::Char('j'), _) => {
                self.scroll = (self.scroll + 1).min(self.rows.len().saturating_sub(1));
            }
            (KeyCode::Char('k'), _) => self.scroll = self.scroll.saturating_sub(1),
            _ => {}
        }
    }

    pub fn render(&self, files: &[ChangedFile], area: Rect, buf: &mut Buffer) {
        let [tree, diff] =
            Layout::horizontal([Constraint::Length(TREE_WIDTH), Constraint::Min(0)]).areas(area);
        let [old, new] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Min(0)]).areas(diff);
        self.render_tree(files, tree, buf);
        let selected = files.get(self.selected);
        let old_title = selected.map_or(String::new(), |f| {
            f.previous_path.clone().unwrap_or_else(|| f.path.clone())
        });
        let new_title = selected.map_or(String::new(), |f| f.path.clone());
        self.render_side(selected, Panel::Old, &old_title, old, buf);
        self.render_side(selected, Panel::New, &new_title, new, buf);
    }

    fn panel(&self, panel: Panel, title: &str) -> Block<'static> {
        let color = if self.focus == panel {
            theme::TEMPLATE.hi
        } else {
            theme::TEMPLATE.border
        };
        Block::bordered()
            .style(theme::on_bg(color))
            .title(format!(" {title} "))
    }

    fn render_tree(&self, files: &[ChangedFile], area: Rect, buf: &mut Buffer) {
        let block = self.panel(Panel::Tree, "Files");
        let inner = block.inner(area).inner(Margin::new(1, 0));
        block.render(area, buf);
        let rows = tree_rows(files);
        if rows.is_empty() {
            Paragraph::new(Line::styled(
                "None",
                theme::on_bg(theme::TEMPLATE.placeholder),
            ))
            .style(theme::base())
            .render(inner, buf);
            return;
        }
        let at = rows
            .iter()
            .position(|r| r.file == Some(self.selected))
            .unwrap_or(0);
        let offset = at.saturating_sub((inner.height as usize).saturating_sub(1));
        let lines: Vec<Line<'static>> = rows
            .into_iter()
            .skip(offset)
            .map(|row| {
                let color = match row.file.map(|i| files[i].status) {
                    Some(FileStatus::Added) => theme::TEMPLATE.success,
                    Some(FileStatus::Removed) => theme::TEMPLATE.error,
                    _ => theme::TEMPLATE.text,
                };
                let line = Line::styled(
                    format!("{}{}", "  ".repeat(row.depth), row.name),
                    theme::on_bg(color),
                );
                if row.file == Some(self.selected) {
                    theme::highlighted(line)
                } else {
                    line
                }
            })
            .collect();
        Paragraph::new(lines)
            .style(theme::base())
            .render(inner, buf);
    }

    fn render_side(
        &self,
        file: Option<&ChangedFile>,
        panel: Panel,
        title: &str,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let block = self.panel(panel, title);
        let inner = block.inner(area).inner(Margin::new(1, 0));
        block.render(area, buf);
        let placeholder = theme::on_bg(theme::TEMPLATE.placeholder);
        let lines: Vec<Line<'static>> = match file {
            None => Vec::new(),
            Some(f) if f.patch.is_none() => vec![Line::styled("No diff available", placeholder)],
            Some(_) => self
                .rows
                .iter()
                .skip(self.scroll)
                .take(inner.height as usize)
                .map(|row| match row {
                    Row::Hunk(header) => Line::styled(header.clone(), placeholder),
                    Row::Lines { left, right } => {
                        let side = if panel == Panel::Old { left } else { right };
                        match side {
                            None => Line::raw(""),
                            Some(s) => {
                                let color = match s.kind {
                                    Kind::Context => theme::TEMPLATE.text,
                                    Kind::Removed => theme::TEMPLATE.error,
                                    Kind::Added => theme::TEMPLATE.success,
                                };
                                Line::from(vec![
                                    Span::styled(format!("{:>4} ", s.number), placeholder),
                                    Span::styled(s.text.clone(), theme::on_bg(color)),
                                ])
                            }
                        }
                    }
                })
                .collect(),
        };
        Paragraph::new(lines)
            .style(theme::base())
            .render(inner, buf);
    }
}

/// The split rows of a file's patch, none without a file or a patch.
fn rows_of(file: Option<&ChangedFile>) -> Vec<Row> {
    file.and_then(|f| f.patch.as_deref())
        .map(diff::split_rows)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::files::FileStatus;
    use crate::testkit::render_buffer;
    use crate::view::theme;

    fn file(path: &str, status: FileStatus, patch: Option<&str>) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            previous_path: None,
            status,
            additions: 0,
            deletions: 0,
            patch: patch.map(String::from),
        }
    }

    fn files() -> Vec<ChangedFile> {
        vec![
            file(
                "src/main.rs",
                FileStatus::Modified,
                Some("@@ -1,2 +1,2 @@\n fn main() {\n-    old();\n+    new();\n"),
            ),
            file(
                "README.md",
                FileStatus::Added,
                Some("@@ -0,0 +1 @@\n+# Title\n"),
            ),
            ChangedFile {
                previous_path: Some("docs/old.md".into()),
                ..file("docs/new.md", FileStatus::Renamed, None)
            },
            file(
                "src/view/a.rs",
                FileStatus::Removed,
                Some("@@ -1 +0,0 @@\n-gone\n"),
            ),
        ]
    }

    fn rows(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// The first cell below the title row where `text` starts.
    fn cell(buf: &Buffer, text: &str) -> (u16, u16) {
        for (y, row) in rows(buf).iter().enumerate().skip(1) {
            if let Some(x) = row.find(text) {
                return (row[..x].chars().count() as u16, y as u16);
            }
        }
        panic!("{text} not drawn");
    }

    #[test]
    /// TU-R-074 — files sort by path, a directory line precedes its entries, depth follows the path.
    fn ut_tree_rows() {
        let row = |depth, name: &str, file| TreeRow {
            depth,
            name: name.into(),
            file,
        };
        assert_eq!(
            tree_rows(&files()),
            vec![
                row(0, "README.md", Some(1)),
                row(0, "docs/", None),
                row(1, "new.md", Some(2)),
                row(0, "src/", None),
                row(1, "main.rs", Some(0)),
                row(1, "view/", None),
                row(2, "a.rs", Some(3)),
            ]
        );
        assert!(tree_rows(&[]).is_empty());
    }

    #[test]
    /// TU-R-074, TU-R-075 — three titled panels, the tree focused and its first file selected; j/k move over files only and show that file's diff; Tab cycles the focus; j/k on a diff side scroll both, clamped.
    fn ut_keys_and_panels() {
        let files = files();
        let mut s = FilesState::new(&files);
        assert_eq!((s.focus(), s.selected()), (Panel::Tree, 1));
        let buf = render_buffer(100, 12, |f| s.render(&files, f.area(), f.buffer_mut()));
        let lines = rows(&buf);
        assert!(
            lines[0].contains(" Files ")
                && lines[0].contains(" README.md ")
                && lines[0].matches(" README.md ").count() == 2,
            "{lines:?}"
        );
        assert!(
            lines[1].contains("README.md")
                && lines[2].contains("docs/")
                && lines[3].contains("  new.md"),
            "{lines:?}"
        );
        let (x, y) = cell(&buf, "README.md");
        assert_eq!(buf[(x, y)].bg, theme::TEMPLATE.hi_bg, "selected file");
        assert_eq!(buf[(x, y + 1)].fg, theme::TEMPLATE.text, "directory");
        let (x, y) = cell(&buf, "a.rs");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.error, "removed file");
        assert_eq!(buf[(0, 0)].fg, theme::TEMPLATE.hi, "tree border focused");
        assert!(lines[1].contains("@@ -0,0 +1 @@"), "{lines:?}");
        assert!(lines[2].contains("   1 # Title"), "{lines:?}");
        let (x, y) = cell(&buf, "# Title");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.success);
        s.handle_key(&files, KeyCode::Char('j'));
        assert_eq!(s.selected(), 2, "skips the directory line");
        let buf = render_buffer(100, 12, |f| s.render(&files, f.area(), f.buffer_mut()));
        let lines = rows(&buf);
        assert!(
            lines[0].contains(" docs/old.md ") && lines[0].contains(" docs/new.md "),
            "{lines:?}"
        );
        assert_eq!(
            lines
                .iter()
                .map(|l| l.matches("No diff available").count())
                .sum::<usize>(),
            2,
            "{lines:?}"
        );
        s.handle_key(&files, KeyCode::Char('j'));
        assert_eq!(s.selected(), 0);
        let buf = render_buffer(100, 12, |f| s.render(&files, f.area(), f.buffer_mut()));
        let lines = rows(&buf);
        assert!(
            lines[2].contains("   1 fn main() {") && lines[2].matches("fn main() {").count() == 2,
            "{lines:?}"
        );
        assert!(
            lines[3].contains("   2     old();") && lines[3].contains("   2     new();"),
            "{lines:?}"
        );
        let (x, y) = cell(&buf, "old();");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.error);
        let (x, y) = cell(&buf, "new();");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.success);
        s.handle_key(&files, KeyCode::Char('j'));
        s.handle_key(&files, KeyCode::Char('j'));
        assert_eq!(s.selected(), 3, "clamped at the last file");
        for _ in 0..10 {
            s.handle_key(&files, KeyCode::Char('k'));
        }
        assert_eq!(s.selected(), 1, "clamped at the first file");
        s.handle_key(&files, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Old);
        s.handle_key(&files, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::New);
        s.handle_key(&files, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Tree);
        s.handle_key(&files, KeyCode::BackTab);
        assert_eq!(s.focus(), Panel::New);
        let buf = render_buffer(100, 12, |f| s.render(&files, f.area(), f.buffer_mut()));
        assert_eq!(
            buf[(0, 0)].fg,
            theme::TEMPLATE.border,
            "tree border unfocused"
        );
        assert_eq!(buf[(99, 0)].fg, theme::TEMPLATE.hi, "new side focused");
        s.handle_key(&files, KeyCode::Char('j'));
        assert_eq!(s.selected(), 1, "selection untouched");
        let buf = render_buffer(100, 12, |f| s.render(&files, f.area(), f.buffer_mut()));
        let lines = rows(&buf);
        assert!(
            lines[1].contains("   1 # Title") && !lines.iter().any(|l| l.contains("@@ -0,0")),
            "scrolled both: {lines:?}"
        );
        s.handle_key(&files, KeyCode::Char('j'));
        let buf = render_buffer(100, 12, |f| s.render(&files, f.area(), f.buffer_mut()));
        assert!(
            rows(&buf)[1].contains("   1 # Title"),
            "never past the last row"
        );
        s.handle_key(&files, KeyCode::Char('k'));
        s.handle_key(&files, KeyCode::Char('k'));
        let buf = render_buffer(100, 12, |f| s.render(&files, f.area(), f.buffer_mut()));
        assert!(rows(&buf)[1].contains("@@ -0,0"), "back at the top");
    }

    #[test]
    /// TU-E-033 — with no files the tree reads `None`, the diff sides stay empty and keys do nothing.
    fn ut_no_files() {
        let mut s = FilesState::new(&[]);
        for code in [KeyCode::Char('j'), KeyCode::Char('k'), KeyCode::Tab] {
            s.handle_key(&[], code);
        }
        s.handle_key(&[], KeyCode::Char('j'));
        let buf = render_buffer(100, 8, |f| s.render(&[], f.area(), f.buffer_mut()));
        let lines = rows(&buf);
        assert!(lines[1].contains("None"), "{lines:?}");
        assert!(!lines.iter().any(|l| l.contains("No diff")), "{lines:?}");
    }
}
