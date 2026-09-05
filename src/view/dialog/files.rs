//! The `Files Changed` tab of the pull request overlay: file tree and the selected file's old
//! and new state side by side.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_syntax::Language;
use ferrowl_ui::Border;
use ferrowl_ui::state::{CodeInputFieldState, CodeInputFieldStateBuilder};
use ferrowl_ui::style::SyntaxThemeBuilder;
use ferrowl_ui::traits::{HandleEvents, SetFocus};
use ferrowl_ui::widgets::{CodeInputField, CodeInputFieldBuilder};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, StatefulWidget, Widget};

use crate::diff::{self, Cell};
use crate::github::blob::Blob;
use crate::github::files::{ChangedFile, FileStatus};
use crate::view::theme;

const TREE_WIDTH: u16 = 30;
const LOADING: &str = "Loading file..";

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

/// What is known about a file's content at the head commit.
enum Loaded {
    Loading,
    Failed(String),
    Blob(Blob),
}

/// What the two diff panels show for the selected file.
enum Sides {
    /// No file is selected.
    Empty,
    /// The same one-line notice on both sides.
    Notice {
        text: String,
        error: bool,
    },
    Fields(Box<Fields>),
}

struct Fields {
    old: CodeInputFieldState,
    new: CodeInputFieldState,
}

pub struct FilesState {
    /// Index into the files of the selected one.
    selected: usize,
    focus: Panel,
    /// Content by path, entered when its request is queued.
    blobs: HashMap<String, Loaded>,
    sides: Sides,
    /// A path whose content the caller has yet to request.
    request: Option<String>,
}

impl FilesState {
    /// The first file selected, the tree focused, that file's content requested.
    pub fn new(files: &[ChangedFile]) -> FilesState {
        let selected = tree_rows(files).iter().find_map(|r| r.file).unwrap_or(0);
        let mut state = FilesState {
            selected,
            focus: Panel::Tree,
            blobs: HashMap::new(),
            sides: Sides::Empty,
            request: None,
        };
        state.refresh(files);
        state
    }

    #[cfg(test)]
    pub fn focus(&self) -> Panel {
        self.focus
    }

    #[cfg(test)]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The active lines of the old and the new side, when both show a file.
    #[cfg(test)]
    pub fn active_lines(&self) -> Option<(usize, usize)> {
        match &self.sides {
            Sides::Fields(f) => Some((f.old.active_line(), f.new.active_line())),
            _ => None,
        }
    }

    /// The path whose content the caller must request, once.
    pub fn take_request(&mut self) -> Option<String> {
        self.request.take()
    }

    /// Stores the content that arrived for `path`; ignored for a path not among the files.
    pub fn handle_blob(
        &mut self,
        files: &[ChangedFile],
        path: &str,
        result: Result<Blob, impl ToString>,
    ) {
        if !files.iter().any(|f| f.path == path) {
            return;
        }
        let loaded = match result {
            Ok(blob) => Loaded::Blob(blob),
            Err(e) => Loaded::Failed(e.to_string()),
        };
        self.blobs.insert(path.to_string(), loaded);
        if files.get(self.selected).is_some_and(|f| f.path == path) {
            self.refresh(files);
        }
    }

    pub fn handle_key(&mut self, files: &[ChangedFile], modifiers: KeyModifiers, code: KeyCode) {
        match (code, self.focus) {
            (KeyCode::Tab, _) => {
                self.set_focus(match self.focus {
                    Panel::Tree => Panel::Old,
                    Panel::Old => Panel::New,
                    Panel::New => Panel::Tree,
                });
            }
            (KeyCode::BackTab, _) => {
                self.set_focus(match self.focus {
                    Panel::Tree => Panel::New,
                    Panel::Old => Panel::Tree,
                    Panel::New => Panel::Old,
                });
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
                    self.refresh(files);
                }
            }
            (_, Panel::Old | Panel::New) => {
                let Sides::Fields(fields) = &mut self.sides else {
                    return;
                };
                let Fields { old, new } = fields.as_mut();
                let code = match code {
                    KeyCode::Char('j') => KeyCode::Down,
                    KeyCode::Char('k') => KeyCode::Up,
                    KeyCode::Char('h') => KeyCode::Left,
                    KeyCode::Char('l') => KeyCode::Right,
                    other => other,
                };
                let (focused, other) = if self.focus == Panel::Old {
                    (old, new)
                } else {
                    (new, old)
                };
                // The field's Left and Right wrap to the neighbouring line at a line's ends;
                // a diff side keeps its line, so those presses are dropped here.
                let line_len = focused
                    .lines()
                    .get(focused.active_line())
                    .map_or(0, |l| l.chars().count());
                let at_edge = match code {
                    KeyCode::Left => focused.cursor_col() == 0,
                    KeyCode::Right => focused.cursor_col() + 1 >= line_len,
                    _ => false,
                };
                if !at_edge {
                    focused.handle_events(modifiers, code);
                }
                // The column stays on a character, never past the line's end, so the
                // horizontal scroll always keeps part of the line in view.
                let line_len = focused
                    .lines()
                    .get(focused.active_line())
                    .map_or(0, |l| l.chars().count());
                if focused.cursor_col() + 1 > line_len {
                    focused.set_cursor_col(line_len.saturating_sub(1));
                }
                other.set_active_line(focused.active_line());
                other.set_cursor_col(focused.cursor_col());
            }
            _ => {}
        }
    }

    fn set_focus(&mut self, focus: Panel) {
        self.focus = focus;
        if let Sides::Fields(f) = &mut self.sides {
            f.old.set_focused(focus == Panel::Old);
            f.new.set_focused(focus == Panel::New);
        }
    }

    /// Rebuilds the diff sides for the selected file, queueing its content request the first time.
    fn refresh(&mut self, files: &[ChangedFile]) {
        let Some(file) = files.get(self.selected) else {
            self.sides = Sides::Empty;
            return;
        };
        let notice = |text: &str, error: bool| Sides::Notice {
            text: text.to_string(),
            error,
        };
        let Some(patch) = file.patch.as_deref() else {
            self.sides = notice("No diff available", false);
            return;
        };
        if file.status == FileStatus::Removed {
            self.sides = self.fields(diff::split_file("", patch));
            return;
        }
        self.sides = match self.blobs.get(&file.path) {
            None => {
                self.blobs.insert(file.path.clone(), Loaded::Loading);
                self.request = Some(file.path.clone());
                notice(LOADING, false)
            }
            Some(Loaded::Loading) => notice(LOADING, false),
            Some(Loaded::Failed(message)) => notice(message, true),
            Some(Loaded::Blob(Blob::Binary)) => notice("Binary file", false),
            Some(Loaded::Blob(Blob::TooLarge)) => notice("File too large", false),
            Some(Loaded::Blob(Blob::Text(text))) => self.fields(diff::split_file(text, patch)),
        };
    }

    fn fields(&self, split: diff::Split) -> Sides {
        Sides::Fields(Box::new(Fields {
            old: field(&split.old, self.focus == Panel::Old),
            new: field(&split.new, self.focus == Panel::New),
        }))
    }

    pub fn render(&mut self, files: &[ChangedFile], area: Rect, buf: &mut Buffer) {
        let [tree, diff] =
            Layout::horizontal([Constraint::Length(TREE_WIDTH), Constraint::Min(0)]).areas(area);
        let [old_area, new_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Min(0)]).areas(diff);
        self.render_tree(files, tree, buf);
        let selected = files.get(self.selected);
        let old_title = selected.map_or(String::new(), |f| {
            f.previous_path.clone().unwrap_or_else(|| f.path.clone())
        });
        let new_title = selected.map_or(String::new(), |f| f.path.clone());
        match &mut self.sides {
            Sides::Empty => {
                self.panel(Panel::Old, &old_title).render(old_area, buf);
                self.panel(Panel::New, &new_title).render(new_area, buf);
            }
            Sides::Notice { text, error } => {
                let color = if *error {
                    theme::TEMPLATE.error
                } else {
                    theme::TEMPLATE.placeholder
                };
                let line = Line::styled(text.clone(), theme::on_bg(color));
                for (panel, title, area) in [
                    (Panel::Old, &old_title, old_area),
                    (Panel::New, &new_title, new_area),
                ] {
                    let block = self.panel(panel, title);
                    let inner = block.inner(area).inner(Margin::new(1, 0));
                    block.render(area, buf);
                    Paragraph::new(line.clone())
                        .style(theme::base())
                        .render(inner, buf);
                }
            }
            Sides::Fields(fields) => {
                let Fields { old, new } = fields.as_mut();
                let old_widget = code_field(&old_title);
                let new_widget = code_field(&new_title);
                // The focused side settles its scroll while rendering; the other side then
                // mirrors it so the same row faces on both sides.
                if self.focus == Panel::New {
                    StatefulWidget::render(&new_widget, new_area, buf, new);
                    mirror(new, old);
                    StatefulWidget::render(&old_widget, old_area, buf, old);
                } else {
                    StatefulWidget::render(&old_widget, old_area, buf, old);
                    mirror(old, new);
                    StatefulWidget::render(&new_widget, new_area, buf, new);
                }
            }
        }
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
}

/// A read-only diff field holding the cells, the active line at the top.
fn field(cells: &[Cell], focused: bool) -> CodeInputFieldState {
    let labels: Vec<String> = if cells.is_empty() {
        vec![String::new()]
    } else {
        cells
            .iter()
            .map(|c| c.number.map_or_else(String::new, |n| n.to_string()))
            .collect()
    };
    let text: Vec<&str> = cells.iter().map(|c| c.text.as_str()).collect();
    let mut state = CodeInputFieldStateBuilder::default()
        .vim(false)
        .disabled(true)
        .focused(focused)
        .language(Some(Language::Diff))
        .gutter_labels(Some(labels))
        .build()
        .expect("CodeInputFieldState fields all default");
    state.set_content(&text.join("\n"));
    state.set_active_line(0);
    state.set_cursor_col(0);
    state
}

fn code_field(title: &str) -> CodeInputField {
    let theme = SyntaxThemeBuilder::default()
        .added(theme::on_bg(theme::TEMPLATE.success))
        .removed(theme::on_bg(theme::TEMPLATE.error))
        .meta(theme::on_bg(theme::TEMPLATE.placeholder))
        .build()
        .expect("SyntaxTheme fields all default");
    CodeInputFieldBuilder::default()
        .border(Border::Full(Margin::new(1, 0)))
        .title(Some(title.into()))
        .style(theme::input_field_style())
        .syntax_theme(theme)
        .build()
        .expect("CodeInputField fields all default")
}

fn mirror(from: &CodeInputFieldState, to: &mut CodeInputFieldState) {
    to.set_active_line(from.active_line());
    to.set_cursor_col(from.cursor_col());
    to.set_scroll_offset(from.scroll_offset());
    to.set_h_scroll(from.h_scroll());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::files::FileStatus;
    use crate::testkit::{buffer_rows, render_buffer};
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
                Some("@@ -1,3 +1,3 @@\n fn main() {\n-    old();\n+    new();\n }\n"),
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

    fn draw(s: &mut FilesState, files: &[ChangedFile], height: u16) -> (Vec<String>, Buffer) {
        let buf = render_buffer(100, height, |f| s.render(files, f.area(), f.buffer_mut()));
        (buffer_rows(&buf), buf)
    }

    /// The first cell below the title row where `text` starts.
    fn cell(buf: &Buffer, text: &str) -> (u16, u16) {
        for (y, row) in buffer_rows(buf).iter().enumerate().skip(1) {
            if let Some(x) = row.find(text) {
                return (row[..x].chars().count() as u16, y as u16);
            }
        }
        panic!("{text} not drawn");
    }

    fn count(rows: &[String], text: &str) -> usize {
        rows.iter().map(|r| r.matches(text).count()).sum()
    }

    fn text(content: &str) -> Result<Blob, String> {
        Ok(Blob::Text(content.into()))
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
    /// TU-R-074, TU-R-075, TU-R-076, TU-E-031, TU-E-037 — three titled panels, the tree focused and its first file selected and requested; the sides read `Loading file..` until the content arrives, then show the whole file with markers, gutter numbers, colors and filler rows; j/k move over files only, requesting each once; a file without a patch reads `No diff available` and a removed file needs no request; Tab cycles the focus.
    fn ut_keys_and_panels() {
        let files = files();
        let mut s = FilesState::new(&files);
        assert_eq!((s.focus(), s.selected()), (Panel::Tree, 1));
        assert_eq!(s.take_request().as_deref(), Some("README.md"));
        assert_eq!(s.take_request(), None, "requested once");
        let (rows, buf) = draw(&mut s, &files, 12);
        assert!(
            rows[0].contains(" Files ") && rows[0].matches(" README.md ").count() == 2,
            "{rows:?}"
        );
        assert!(
            rows[1].contains("README.md")
                && rows[2].contains("docs/")
                && rows[3].contains("  new.md"),
            "{rows:?}"
        );
        let (x, y) = cell(&buf, "README.md");
        assert_eq!(buf[(x, y)].bg, theme::TEMPLATE.hi_bg, "selected file");
        assert_eq!(buf[(x, y + 1)].fg, theme::TEMPLATE.text, "directory");
        let (x, y) = cell(&buf, "a.rs");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.error, "removed file");
        assert_eq!(buf[(0, 0)].fg, theme::TEMPLATE.hi, "tree border focused");
        assert_eq!(count(&rows, LOADING), 2, "{rows:?}");
        let (x, y) = cell(&buf, LOADING);
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.placeholder);

        s.handle_blob(&files, "README.md", text("# Title\n"));
        let (rows, buf) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "1 +# Title"), 1, "{rows:?}");
        let (x, y) = cell(&buf, "+# Title");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.success);
        let old_inner = &rows[y as usize][TREE_WIDTH as usize..65];
        assert!(
            old_inner.trim_matches(['│', ' ']).is_empty(),
            "filler row blank: {old_inner:?}"
        );

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.selected(), 2, "skips the directory line");
        assert_eq!(s.take_request(), None);
        let (rows, _) = draw(&mut s, &files, 12);
        assert!(
            rows[0].contains(" docs/old.md ") && rows[0].contains(" docs/new.md "),
            "{rows:?}"
        );
        assert_eq!(count(&rows, "No diff available"), 2, "{rows:?}");

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.selected(), 0);
        assert_eq!(s.take_request().as_deref(), Some("src/main.rs"));
        s.handle_blob(&files, "src/main.rs", text("fn main() {\n    new();\n}\n"));
        let (rows, buf) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "1  fn main() {"), 2, "{rows:?}");
        assert!(
            rows.iter()
                .any(|r| r.contains("2 -    old();") && r.contains("2 +    new();")),
            "{rows:?}"
        );
        assert_eq!(count(&rows, "3  }"), 2, "{rows:?}");
        let (x, y) = cell(&buf, "-    old();");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.error);
        let (x, y) = cell(&buf, "+    new();");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.success);
        let (x, y) = cell(&buf, " fn main() {");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.text);

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.selected(), 3, "clamped at the last file");
        assert_eq!(s.take_request(), None, "a removed file needs no content");
        let (rows, buf) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "1 -gone"), 1, "{rows:?}");
        let (x, y) = cell(&buf, "-gone");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.error);
        assert!(
            rows[y as usize][65..].trim_matches(['│', ' ']).is_empty(),
            "new side blank: {rows:?}"
        );

        for _ in 0..10 {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        }
        assert_eq!(s.selected(), 1, "clamped at the first file");
        assert_eq!(s.take_request(), None, "README.md was requested before");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Old);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::New);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Tree);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::BackTab);
        assert_eq!(s.focus(), Panel::New);
        let (_, buf) = draw(&mut s, &files, 12);
        assert_eq!(
            buf[(0, 0)].fg,
            theme::TEMPLATE.border,
            "tree border unfocused"
        );
        assert_eq!(buf[(99, 0)].fg, theme::TEMPLATE.hi, "new side focused");
        assert_eq!(
            buf[(TREE_WIDTH, 0)].fg,
            theme::TEMPLATE.border,
            "old side unfocused"
        );
    }

    #[test]
    /// TU-R-075 — on a focused diff side j and k move its active line and the other side mirrors it; the scroll offset is mirrored too so the same row faces on both sides; the selection stays.
    fn ut_side_navigation_mirrors() {
        let files = files();
        let mut s = FilesState::new(&files);
        s.take_request();
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.take_request().as_deref(), Some("src/main.rs"));
        s.handle_blob(&files, "src/main.rs", text("fn main() {\n    new();\n}\n"));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::BackTab);
        assert_eq!(s.focus(), Panel::New);
        assert_eq!(s.active_lines(), Some((0, 0)));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.active_lines(), Some((1, 1)));
        assert_eq!(s.selected(), 0, "selection untouched");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Down);
        assert_eq!(s.active_lines(), Some((2, 2)));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.active_lines(), Some((2, 2)), "never past the last row");
        let (rows, _) = draw(&mut s, &files, 4);
        assert_eq!(count(&rows, "fn main() {"), 0, "scrolled both: {rows:?}");
        assert!(
            rows.iter()
                .any(|r| r.contains("-    old();") && r.contains("+    new();")),
            "{rows:?}"
        );
        assert_eq!(count(&rows, "3  }"), 2, "{rows:?}");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Up);
        assert_eq!(s.active_lines(), Some((0, 0)));
        let (rows, _) = draw(&mut s, &files, 4);
        assert_eq!(
            count(&rows, "1  fn main() {"),
            2,
            "back at the top: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::BackTab);
        assert_eq!(s.focus(), Panel::Old);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.active_lines(), Some((1, 1)));
    }

    #[test]
    /// TU-R-075, TU-E-041, TU-E-042 — h/l and Left/Right move the focused side's column, the side scrolls horizontally to show it and the other side mirrors both; h at the first column and l at the line end do nothing and never wrap to another line; a shorter line clamps the column.
    fn ut_horizontal_scroll_mirrors() {
        let long = "x".repeat(60);
        let files = vec![file(
            "a.rs",
            FileStatus::Modified,
            Some(&format!("@@ -1,2 +1,2 @@\n {long}END\n-short\n+tiny\n")),
        )];
        let mut s = FilesState::new(&files);
        s.take_request();
        s.handle_blob(&files, "a.rs", text(&format!("{long}END\ntiny\n")));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Old);
        let (rows, _) = draw(&mut s, &files, 6);
        assert_eq!(count(&rows, "END"), 0, "overflow cut: {rows:?}");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('h'));
        assert_eq!(
            s.active_lines(),
            Some((0, 0)),
            "h at the first column stays"
        );
        for _ in 0..70 {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('l'));
        }
        assert_eq!(s.active_lines(), Some((0, 0)), "l at the end never wraps");
        let (rows, _) = draw(&mut s, &files, 6);
        assert_eq!(count(&rows, "END"), 2, "both sides scrolled: {rows:?}");
        assert_eq!(count(&rows, "1  x"), 0, "{rows:?}");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.active_lines(), Some((1, 1)));
        let (rows, _) = draw(&mut s, &files, 6);
        assert!(
            rows.iter().any(|r| r.contains("2 t")),
            "clamped to the last character, the shorter facing line scrolled out: {rows:?}"
        );
        for _ in 0..10 {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Left);
        }
        assert_eq!(s.active_lines(), Some((1, 1)), "Left never wraps up");
        let (rows, _) = draw(&mut s, &files, 6);
        assert!(
            rows.iter()
                .any(|r| r.contains("2 -short") && r.contains("2 +tiny")),
            "scrolled back: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Right);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::BackTab);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::BackTab);
        assert_eq!(s.focus(), Panel::New);
        for _ in 0..70 {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Right);
        }
        let (rows, _) = draw(&mut s, &files, 6);
        assert_eq!(
            count(&rows, "END"),
            2,
            "mirrored from the new side: {rows:?}"
        );
    }

    #[test]
    /// TU-R-076, TU-E-035, TU-E-036, TU-E-039, TU-E-040 — a failed request shows its message in the error color, a binary file `Binary file`, a too large one `File too large`; content for an unknown path is discarded; focus on a side showing a notice still colors its border and ignores navigation keys.
    fn ut_notices() {
        let files = vec![
            file("a", FileStatus::Modified, Some("@@ -1 +1 @@\n-x\n+y\n")),
            file("b", FileStatus::Modified, Some("@@ -1 +1 @@\n-x\n+y\n")),
            file("c", FileStatus::Modified, Some("@@ -1 +1 @@\n-x\n+y\n")),
        ];
        let mut s = FilesState::new(&files);
        assert_eq!(s.take_request().as_deref(), Some("a"));
        s.handle_blob(&files, "a", Err::<Blob, _>("github: HTTP 502"));
        let (rows, buf) = draw(&mut s, &files, 6);
        assert_eq!(count(&rows, "github: HTTP 502"), 2, "{rows:?}");
        let (x, y) = cell(&buf, "github: HTTP 502");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.error);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.focus(), Panel::Old);
        let (_, buf) = draw(&mut s, &files, 6);
        assert_eq!(
            buf[(TREE_WIDTH, 0)].fg,
            theme::TEMPLATE.hi,
            "old side focused"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::BackTab);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.take_request().as_deref(), Some("b"));
        s.handle_blob(&files, "b", Ok::<_, String>(Blob::Binary));
        let (rows, _) = draw(&mut s, &files, 6);
        assert_eq!(count(&rows, "Binary file"), 2, "{rows:?}");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.take_request().as_deref(), Some("c"));
        s.handle_blob(&files, "zzz", text("ignored"));
        let (rows, _) = draw(&mut s, &files, 6);
        assert_eq!(count(&rows, LOADING), 2, "still loading: {rows:?}");
        s.handle_blob(&files, "c", Ok::<_, String>(Blob::TooLarge));
        let (rows, _) = draw(&mut s, &files, 6);
        assert_eq!(count(&rows, "File too large"), 2, "{rows:?}");
        s.handle_blob(&files, "a", text("y\n"));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        assert_eq!(s.take_request(), None);
        let (rows, _) = draw(&mut s, &files, 6);
        assert!(
            rows.iter()
                .any(|r| r.contains("1 -x") && r.contains("1 +y")),
            "{rows:?}"
        );
    }

    #[test]
    /// TU-E-033 — with no files the tree reads `None`, the diff sides stay empty, nothing is requested and keys do nothing.
    fn ut_no_files() {
        let mut s = FilesState::new(&[]);
        assert_eq!(s.take_request(), None);
        for code in [KeyCode::Char('j'), KeyCode::Char('k'), KeyCode::Tab] {
            s.handle_key(&[], KeyModifiers::NONE, code);
        }
        s.handle_key(&[], KeyModifiers::NONE, KeyCode::Char('j'));
        s.handle_blob(&[], "x", text("x"));
        let (rows, _) = draw(&mut s, &[], 8);
        assert!(rows[1].contains("None"), "{rows:?}");
        assert!(
            !rows
                .iter()
                .any(|l| l.contains("No diff") || l.contains(LOADING)),
            "{rows:?}"
        );
    }
}
