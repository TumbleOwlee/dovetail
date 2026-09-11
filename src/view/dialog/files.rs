//! The `Files Changed` tab of the pull request overlay: file tree and the selected file's
//! patch in the diff widget.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::state::{
    DiffLayout, DiffViewState, DiffViewStateBuilder, FileStatus as TreeStatus, FileTreeState,
    FileTreeStateBuilder, Side,
};
use ferrowl_ui::traits::{HandleEvents, SetFocus};
use ferrowl_ui::widgets::{DiffViewBuilder, FileTreeBuilder};
use ferrowl_ui::{Border, EventResult};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, StatefulWidget, Widget};

use crate::github::blob::Blob;
use crate::github::files::{ChangedFile, FileStatus};
use crate::github::pull::ReviewThread;
use crate::view::dialog::review::{PanelEvent, ReviewPanel};
use crate::view::theme;

const TREE_WIDTH: u16 = 30;

/// The focusable panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Tree,
    Diff,
    /// The comment panel under the diff.
    Comment,
}

/// The tree widget in the tab's style: bordered, titled `Files`.
fn tree_widget() -> ferrowl_ui::widgets::FileTree {
    FileTreeBuilder::default()
        .border(Border::Full(Margin::new(1, 0)))
        .title(Some(" Files ".into()))
        .style(theme::input_field_style())
        .syntax_theme(theme::status_theme())
        .build()
        .expect("FileTree fields all default")
}

/// The tree widget's status marker for a changed file; `None` where it has no marker.
fn tree_status(status: FileStatus) -> Option<TreeStatus> {
    match status {
        FileStatus::Added => Some(TreeStatus::Added),
        FileStatus::Removed => Some(TreeStatus::Removed),
        FileStatus::Modified => Some(TreeStatus::Modified),
        FileStatus::Renamed | FileStatus::Copied | FileStatus::Changed | FileStatus::Unchanged => {
            None
        }
    }
}

/// What is known about a file's content at the head commit.
enum Loaded {
    Loading,
    Failed,
    Blob(Blob),
}

/// What the diff panel shows for the selected file.
enum Shown {
    /// No file is selected.
    Empty,
    /// A one-line notice instead of a diff.
    Notice(String),
    View(Box<DiffViewState>),
}

pub struct FilesState {
    tree: FileTreeState,
    /// Index into the files of the one whose diff shows.
    selected: Option<usize>,
    focus: Panel,
    /// Content by path, entered when its request is queued.
    blobs: HashMap<String, Loaded>,
    shown: Shown,
    /// A path whose content the caller has yet to request.
    request: Option<String>,
    /// Review threads and mode; `None` outside the pull request's own diff.
    review: Option<ReviewPanel>,
    /// The tab's chosen diff layout, kept when the selection moves to another file.
    layout: DiffLayout,
    /// The tab's line-wrap choice, kept like the layout; off by default.
    wrap: bool,
}

impl FilesState {
    /// Like [`FilesState::new`], with the pull request's review threads shown and review
    /// mode available.
    pub fn with_review(files: &[ChangedFile], threads: &[ReviewThread]) -> FilesState {
        let mut state = FilesState::new(files);
        state.review = Some(ReviewPanel::new(threads));
        state.sync_marks_for(files);
        state
    }

    /// The first file in tree order selected, the tree focused, that file's content
    /// requested.
    pub fn new(files: &[ChangedFile]) -> FilesState {
        let paths: Vec<(String, Option<TreeStatus>)> = files
            .iter()
            .map(|f| (f.path.clone(), tree_status(f.status)))
            .collect();
        let mut tree = FileTreeStateBuilder::default()
            .paths(paths)
            .build()
            .expect("FileTreeState fields all default");
        tree.set_focused(true);
        // Before any render the tree's page is one row, so stepping the selection below
        // would scroll the top rows out of view; a scratch render first sets a real
        // visible height.
        let area = Rect::new(0, 0, TREE_WIDTH, 128);
        let mut scratch = Buffer::empty(area);
        StatefulWidget::render(&tree_widget(), area, &mut scratch, &mut tree);
        // The tree opens on its first row; step past leading directory rows so a file is
        // selected. Every step descends one visible row, so the total path segment count
        // bounds the walk.
        let mut steps: usize = files.iter().map(|f| f.path.split('/').count()).sum();
        while tree.selected_is_dir() == Some(true) && steps > 0 {
            tree.handle_key(KeyModifiers::NONE, KeyCode::Down);
            steps -= 1;
        }
        let selected = tree
            .selected_path()
            .and_then(|p| files.iter().position(|f| f.path == p));
        let mut state = FilesState {
            tree,
            selected,
            focus: Panel::Tree,
            blobs: HashMap::new(),
            shown: Shown::Empty,
            request: None,
            review: None,
            layout: DiffLayout::Split,
            wrap: false,
        };
        state.refresh(files);
        state
    }

    pub fn focus(&self) -> Panel {
        self.focus
    }

    pub fn review(&self) -> Option<&ReviewPanel> {
        self.review.as_ref()
    }

    pub fn review_mut(&mut self) -> Option<&mut ReviewPanel> {
        self.review.as_mut()
    }

    /// The selected file's path.
    fn path<'f>(&self, files: &'f [ChangedFile]) -> Option<&'f str> {
        self.selected
            .and_then(|i| files.get(i))
            .map(|f| f.path.as_str())
    }

    /// The file lines the diff selection touches, per side.
    fn touched(&self) -> (Vec<usize>, Vec<usize>) {
        let Shown::View(state) = &self.shown else {
            return (Vec::new(), Vec::new());
        };
        let Some(rows) = state.selected_rows() else {
            return (Vec::new(), Vec::new());
        };
        let mut old = Vec::new();
        let mut new = Vec::new();
        for index in rows {
            if let Some(info) = state.row(index) {
                old.extend(info.old_line);
                new.extend(info.new_line);
            }
        }
        (old, new)
    }

    /// Indices of the review threads the diff selection touches.
    fn visible_threads(&self, files: &[ChangedFile]) -> Vec<usize> {
        let (Some(review), Some(path)) = (&self.review, self.path(files)) else {
            return Vec::new();
        };
        let (old, new) = self.touched();
        review.visible(path, &old, &new)
    }

    /// Repaints the gutter marks of the shown diff from the review threads.
    pub fn sync_marks_for(&mut self, files: &[ChangedFile]) {
        let marks = match (&self.review, self.path(files)) {
            (Some(review), Some(path)) => review.marks(path),
            _ => return,
        };
        if let Shown::View(state) = &mut self.shown {
            state.set_marked_ranges(marks);
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> Option<usize> {
        self.selected
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
            Err(_) => Loaded::Failed,
        };
        self.blobs.insert(path.to_string(), loaded);
        if self
            .selected
            .and_then(|i| files.get(i))
            .is_some_and(|f| f.path == path)
        {
            self.refresh(files);
        }
    }

    /// Routes one key; reports whether the tab consumed it, so an unconsumed Esc can
    /// close the overlay.
    pub fn handle_key(
        &mut self,
        files: &[ChangedFile],
        modifiers: KeyModifiers,
        code: KeyCode,
    ) -> bool {
        match (code, self.focus) {
            (KeyCode::Tab, _) => {
                self.cycle_focus(files, true);
                true
            }
            (KeyCode::BackTab, _) => {
                self.cycle_focus(files, false);
                true
            }
            (_, Panel::Tree) => match self.tree.handle_key(modifiers, code) {
                Some(_) => {
                    self.follow_tree(files);
                    true
                }
                None => false,
            },
            (KeyCode::Char('c'), Panel::Diff)
                if modifiers == KeyModifiers::NONE && self.review.is_some() =>
            {
                self.start_comment(files);
                true
            }
            (KeyCode::Char('t'), Panel::Diff) if modifiers == KeyModifiers::NONE => {
                match &mut self.shown {
                    // The widget binds the layout toggle to Ctrl+T, which the overlay's
                    // tab-switch chord consumes first; `t` stands in for it here.
                    Shown::View(state) => {
                        state.handle_events(KeyModifiers::CONTROL, KeyCode::Char('t'));
                        self.layout = state.layout();
                        true
                    }
                    _ => false,
                }
            }
            (KeyCode::Char('w'), Panel::Diff) if modifiers == KeyModifiers::NONE => {
                match self.shown {
                    // The wrap option is builder-set, so the toggle rebuilds the shown
                    // diff.
                    Shown::View(_) => {
                        self.wrap = !self.wrap;
                        self.refresh(files);
                        true
                    }
                    _ => false,
                }
            }
            (_, Panel::Diff) => match &mut self.shown {
                Shown::View(state) => {
                    matches!(state.handle_events(modifiers, code), EventResult::Consumed)
                }
                _ => false,
            },
            (_, Panel::Comment) => {
                let visible = self.visible_threads(files);
                let Some(review) = &mut self.review else {
                    return false;
                };
                match review.handle_key(&visible, modifiers, code) {
                    PanelEvent::Consumed => {
                        self.sync_marks_for(files);
                        if !self.panel_shown(files) {
                            self.set_focus(files, Panel::Diff);
                        }
                        true
                    }
                    PanelEvent::ToDiff => {
                        if code == KeyCode::Esc {
                            self.set_focus(files, Panel::Diff);
                            true
                        } else {
                            false
                        }
                    }
                }
            }
        }
    }

    /// Whether the comment panel takes rows under the diff.
    fn panel_shown(&self, files: &[ChangedFile]) -> bool {
        let visible = self.visible_threads(files);
        self.review
            .as_ref()
            .is_some_and(|r| r.panel_shown(&visible))
    }

    /// Moves the focus tree → diff → thread box → reply editor (while one is open) →
    /// tree, the comment stops only while the panel is shown.
    fn cycle_focus(&mut self, files: &[ChangedFile], forward: bool) {
        let visible = self.visible_threads(files);
        if self.focus == Panel::Comment
            && let Some(review) = &mut self.review
            && review.advance(&visible, forward)
        {
            self.set_focus(files, Panel::Comment);
            return;
        }
        let comment = self.panel_shown(files);
        let next = match (self.focus, forward) {
            (Panel::Tree, true) => Panel::Diff,
            (Panel::Diff, true) if comment => Panel::Comment,
            (Panel::Diff, true) | (Panel::Comment, true) => Panel::Tree,
            (Panel::Tree, false) if comment => Panel::Comment,
            (Panel::Tree, false) => Panel::Diff,
            (Panel::Diff, false) => Panel::Tree,
            (Panel::Comment, false) => Panel::Diff,
        };
        if next == Panel::Comment
            && let Some(review) = &mut self.review
        {
            review.enter(&visible, forward);
        }
        self.set_focus(files, next);
    }

    /// Opens a new draft comment on the diff selection of the focused side; consumed
    /// with a notice when review mode or a line is missing.
    fn start_comment(&mut self, files: &[ChangedFile]) {
        let path = self.path(files).map(str::to_string);
        let (old, new) = self.touched();
        let Some(review) = &mut self.review else {
            return;
        };
        if !review.active {
            review.set_notice("no review: run :review");
            return;
        }
        if review.submitting {
            review.set_notice("review busy");
            return;
        }
        let Shown::View(state) = &mut self.shown else {
            review.set_notice("no line to comment");
            return;
        };
        // The new side wherever the selection has file lines there; removed-only
        // selections fall back to the old side.
        let (lines, side) = if new.is_empty() {
            (old, Side::Old)
        } else {
            (new, Side::New)
        };
        let (Some(first), Some(last), Some(path)) = (lines.iter().min(), lines.iter().max(), path)
        else {
            review.set_notice("no line to comment");
            return;
        };
        state.handle_events(KeyModifiers::NONE, KeyCode::Esc);
        review.open_draft(&path, side, *first..=*last);
        self.set_focus(files, Panel::Comment);
        self.sync_marks_for(files);
    }

    /// Shows the diff of the file the tree selection came to rest on; a directory row
    /// keeps the shown diff.
    fn follow_tree(&mut self, files: &[ChangedFile]) {
        if self.tree.selected_is_dir() != Some(false) {
            return;
        }
        let index = self
            .tree
            .selected_path()
            .and_then(|p| files.iter().position(|f| f.path == p));
        if index.is_some() && index != self.selected {
            self.selected = index;
            self.refresh(files);
        }
    }

    fn set_focus(&mut self, files: &[ChangedFile], focus: Panel) {
        let visible = self.visible_threads(files);
        self.focus = focus;
        self.tree.set_focused(focus == Panel::Tree);
        if let Shown::View(state) = &mut self.shown {
            state.set_focused(focus == Panel::Diff);
        }
        if let Some(review) = &mut self.review {
            review.set_focused(focus == Panel::Comment, &visible);
        }
        self.sync_marks_for(files);
    }

    /// Rebuilds the diff panel for the selected file, queueing its content request the
    /// first time.
    fn refresh(&mut self, files: &[ChangedFile]) {
        let Some(file) = self.selected.and_then(|i| files.get(i)) else {
            self.shown = Shown::Empty;
            return;
        };
        let Some(patch) = file.patch.as_deref() else {
            self.shown = Shown::Notice("No diff available".to_string());
            return;
        };
        if file.status != FileStatus::Removed && !self.blobs.contains_key(&file.path) {
            self.blobs.insert(file.path.clone(), Loaded::Loading);
            self.request = Some(file.path.clone());
        }
        let builder = DiffViewStateBuilder::default()
            .layout(self.layout)
            .wrap(self.wrap)
            .clone();
        let mut state = match self.blobs.get(&file.path) {
            Some(Loaded::Blob(Blob::Text(text))) => builder
                .build_with_diff_and_file(patch, text)
                .expect("DiffViewState fields all default"),
            _ => builder
                .build_with_diff(patch)
                .expect("DiffViewState fields all default"),
        };
        state.set_focused(self.focus == Panel::Diff);
        self.shown = Shown::View(Box::new(state));
        self.sync_marks_for(files);
    }

    pub fn render(&mut self, files: &[ChangedFile], area: Rect, buf: &mut Buffer) {
        let [tree, right] =
            Layout::horizontal([Constraint::Length(TREE_WIDTH), Constraint::Min(0)]).areas(area);
        StatefulWidget::render(&tree_widget(), tree, buf, &mut self.tree);
        let visible = self.visible_threads(files);
        let panel = self
            .review
            .as_ref()
            .is_some_and(|r| r.panel_shown(&visible));
        let (diff, comment) = if panel {
            let reply = self.review.as_ref().is_some_and(|r| r.reply_open(&visible));
            let height = if reply {
                // Half the tab for the thread box plus the reply editor's eight rows.
                (area.height / 2).clamp(3, 20) + 8
            } else {
                (area.height / 3).clamp(3, 12) + 5
            }
            .min(area.height.saturating_sub(3));
            let [diff, comment] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(height)]).areas(right);
            (diff, Some(comment))
        } else {
            (right, None)
        };
        let title = self
            .selected
            .and_then(|i| files.get(i))
            .map_or(String::new(), |f| {
                f.previous_path
                    .as_deref()
                    .map_or_else(|| f.path.clone(), |p| format!("{p} → {}", f.path))
            });
        match &mut self.shown {
            Shown::Empty => {
                self.panel(&title).render(diff, buf);
            }
            Shown::Notice(text) => {
                let line = Line::styled(text.clone(), theme::on_bg(theme::TEMPLATE.placeholder));
                let block = self.panel(&title);
                let inner = block.inner(diff).inner(Margin::new(1, 0));
                block.render(diff, buf);
                Paragraph::new(line).style(theme::base()).render(inner, buf);
            }
            Shown::View(state) => {
                let widget = DiffViewBuilder::default()
                    .border(Border::Full(Margin::new(1, 0)))
                    .title(Some(format!(" {title} ").into()))
                    .style(theme::diff_view_style())
                    .build()
                    .expect("DiffView fields all default");
                StatefulWidget::render(&widget, diff, buf, state.as_mut());
            }
        }
        if let (Some(review), Some(comment)) = (&mut self.review, comment) {
            review.render(&visible, self.focus == Panel::Comment, comment, buf);
        }
    }

    fn panel(&self, title: &str) -> Block<'static> {
        let color = if self.focus == Panel::Diff {
            theme::TEMPLATE.hi
        } else {
            theme::TEMPLATE.border
        };
        Block::bordered()
            .style(theme::on_bg(color))
            .title(format!(" {title} "))
    }
}

#[cfg(test)]
mod tests {
    use ferrowl_ui::COLOR_SCHEME;

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
    /// TU-R-074, TU-R-076, TU-E-031, TU-E-037 — the tree widget at the left with the files in sorted path order, status markers and colors, the selected row highlighted; the diff widget at the right shows the selected file's patch immediately, hunk-only, added rows on the widget's added band, a filler side blank; the selection moving over the tree follows onto files, a directory row keeping the shown diff; each file's content is requested once; a file without a patch reads `No diff available`; a renamed file titles the panel `old → new`; a removed file requests nothing and keeps its patch view.
    fn ut_tree_and_diff_panel() {
        let files = files();
        let mut s = FilesState::new(&files);
        assert_eq!((s.focus(), s.selected()), (Panel::Tree, Some(2)));
        assert_eq!(s.take_request(), None, "no patch, no request");
        let (rows, buf) = draw(&mut s, &files, 12);
        assert!(
            rows[0].contains(" Files ") && rows[0].contains(" docs/old.md → docs/new.md "),
            "{rows:?}"
        );
        assert!(
            rows[1].contains("docs")
                && rows[2].contains("new.md")
                && rows[3].contains("src")
                && rows[7].contains("README.md"),
            "directories before files: {rows:?}"
        );
        assert_eq!(count(&rows, "No diff available"), 1, "{rows:?}");
        let (x, y) = cell(&buf, "new.md");
        assert_eq!(buf[(x, y)].bg, theme::TEMPLATE.hi_bg, "selected row");
        let (x, y) = cell(&buf, "README.md");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.success, "added file");
        let (x, y) = cell(&buf, "a.rs");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.error, "removed file");
        let (x, y) = cell(&buf, "main.rs");
        assert_eq!(buf[(x, y)].fg, theme::TEMPLATE.text, "modified file");

        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j')));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.selected(), Some(2), "directory rows keep the diff");
        let (rows, _) = draw(&mut s, &files, 12);
        assert!(rows[0].contains(" docs/old.md → docs/new.md "), "{rows:?}");

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(
            s.selected(),
            Some(3),
            "past the src and view directory rows"
        );
        assert_eq!(s.take_request(), None, "a removed file needs no content");
        let (rows, buf) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "gone"), 1, "{rows:?}");
        let (x, y) = cell(&buf, "gone");
        assert_eq!(buf[(x, y)].bg, COLOR_SCHEME.diff_removed);

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.selected(), Some(0));
        assert_eq!(s.take_request().as_deref(), Some("src/main.rs"));
        assert_eq!(s.take_request(), None, "requested once");
        let (rows, buf) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "fn main() {"), 2, "both panes: {rows:?}");
        assert!(
            rows.iter()
                .any(|r| r.matches("old();").count() == 1 && r.matches("new();").count() == 1),
            "changed row pairs: {rows:?}"
        );
        let (x, y) = cell(&buf, "old();");
        assert_eq!(
            buf[(x, y)].bg,
            COLOR_SCHEME.diff_removed_word,
            "changed word emphasized"
        );
        assert_eq!(
            buf[(x - 1, y)].bg,
            COLOR_SCHEME.diff_removed,
            "removed band"
        );
        let (x, y) = cell(&buf, "new();");
        assert_eq!(buf[(x, y)].bg, COLOR_SCHEME.diff_added_word);
        assert_eq!(buf[(x - 1, y)].bg, COLOR_SCHEME.diff_added);

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.selected(), Some(1));
        assert_eq!(s.take_request().as_deref(), Some("README.md"));
        let (rows, buf) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "# Title"), 1, "patch shown while loading");
        let (x, y) = cell(&buf, "# Title");
        assert_eq!(buf[(x, y)].bg, COLOR_SCHEME.diff_added, "added band");
        let old_inner = &rows[y as usize][(TREE_WIDTH as usize) + 1..64];
        assert!(
            old_inner.trim_matches(['│', ' ']).is_empty(),
            "filler side blank: {old_inner:?}"
        );
        s.handle_blob(&files, "README.md", text("# Title\n"));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "# Title"), 1, "{rows:?}");

        for _ in 0..10 {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        }
        assert_eq!(s.selected(), Some(2), "the last file passed on the way up");
        assert_eq!(s.take_request(), None, "nothing new to request");
    }

    #[test]
    /// TU-R-075, TU-E-040, TU-E-046 — Tab and Shift+Tab toggle the focus between the tree and the diff panel, the focused borders in the highlight color; keys on the focused diff panel go to the widget (`j` consumed); Esc is consumed only to leave the widget's visual mode; on a notice panel navigation keys are not consumed but the border still shows the focus.
    fn ut_focus_keys_and_esc() {
        let files = files();
        let mut s = FilesState::new(&files);
        for _ in 0..4 {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        }
        assert_eq!(s.selected(), Some(0));
        s.take_request();
        s.handle_blob(&files, "src/main.rs", text("fn main() {\n    new();\n}\n"));
        let (_, buf) = draw(&mut s, &files, 12);
        assert_eq!(buf[(0, 0)].fg, theme::TEMPLATE.hi, "tree border focused");
        assert_eq!(
            buf[(TREE_WIDTH, 0)].fg,
            theme::TEMPLATE.border,
            "diff border unfocused"
        );
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab));
        assert_eq!(s.focus(), Panel::Diff);
        let (_, buf) = draw(&mut s, &files, 12);
        assert_eq!(buf[(0, 0)].fg, theme::TEMPLATE.border, "tree unfocused");
        assert_eq!(buf[(TREE_WIDTH, 0)].fg, theme::TEMPLATE.hi, "diff focused");
        assert_eq!(buf[(99, 0)].fg, theme::TEMPLATE.hi, "both pane borders");
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::BackTab));
        assert_eq!(s.focus(), Panel::Tree);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);

        assert!(
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j')),
            "widget consumes j"
        );
        assert!(
            !s.handle_key(&files, KeyModifiers::NONE, KeyCode::Esc),
            "Esc unconsumed outside visual mode"
        );
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('v')));
        assert!(
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Esc),
            "Esc leaves visual mode"
        );
        assert!(!s.handle_key(&files, KeyModifiers::NONE, KeyCode::Esc));

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        for _ in 0..4 {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        }
        assert_eq!(s.selected(), Some(2), "back on the tree");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert!(
            !s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j')),
            "notice panel consumes nothing"
        );
        let (_, buf) = draw(&mut s, &files, 12);
        assert_eq!(
            buf[(TREE_WIDTH, 0)].fg,
            theme::TEMPLATE.hi,
            "notice border focused"
        );
    }

    #[test]
    /// TU-R-075 — `t` on the focused diff panel toggles between the split and unified layouts, split by default; `t` on the focused tree does not; the chosen layout is the tab's: it persists when the selection moves to another file and for content arriving later.
    fn ut_layout_toggle() {
        let files = vec![
            file(
                "a.rs",
                FileStatus::Modified,
                Some("@@ -1,3 +1,3 @@\n fn main() {\n-    old();\n+    new();\n }\n"),
            ),
            file(
                "b.rs",
                FileStatus::Modified,
                Some("@@ -1,3 +1,3 @@\n fn other() {\n-    old();\n+    new();\n }\n"),
            ),
        ];
        let mut s = FilesState::new(&files);
        s.take_request();
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('t'));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(
            count(&rows, "fn main() {"),
            2,
            "tree t is no toggle: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('t')));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "fn main() {"), 1, "unified: {rows:?}");
        assert!(
            rows.iter()
                .any(|r| r.contains("old();") && !r.contains("new();")),
            "sequential rows: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('t'));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "fn main() {"), 2, "back to split: {rows:?}");

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('t'));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.selected(), Some(1));
        s.take_request();
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(
            count(&rows, "fn other() {"),
            1,
            "unified persists across files: {rows:?}"
        );
        s.handle_blob(&files, "b.rs", text("fn other() {\n    new();\n}\n"));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(
            count(&rows, "fn other() {"),
            1,
            "unified survives the arrived content: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('t'));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('k'));
        assert_eq!(s.selected(), Some(0));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(
            count(&rows, "fn main() {"),
            2,
            "split persists back on the first file: {rows:?}"
        );
    }

    #[test]
    /// TU-R-076, TU-E-035, TU-E-036, TU-E-039 — a failed request, a binary file and a too-large file keep the hunk-only patch view; content for an unknown path is discarded; full-file rows appear only after the content arrived as text.
    fn ut_hunk_only_until_text_arrives() {
        let patch = "@@ -2,3 +2,3 @@\n two\n-three\n+drei\n four\n";
        let content = "one\ntwo\ndrei\nfour\nfive\n";
        let files = vec![
            file("a", FileStatus::Modified, Some(patch)),
            file("b", FileStatus::Modified, Some(patch)),
            file("c", FileStatus::Modified, Some(patch)),
            file("d", FileStatus::Modified, Some(patch)),
        ];
        let mut s = FilesState::new(&files);
        assert_eq!(s.take_request().as_deref(), Some("a"));
        s.handle_blob(&files, "a", Err::<Blob, _>("github: HTTP 502"));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "three"), 1, "hunk view stays: {rows:?}");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        s.handle_key(&files, KeyModifiers::CONTROL, KeyCode::Char('f'));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "one"), 0, "no full file to unfold: {rows:?}");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.take_request().as_deref(), Some("b"));
        s.handle_blob(&files, "b", Ok::<_, String>(Blob::Binary));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "three"), 1, "{rows:?}");

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.take_request().as_deref(), Some("c"));
        s.handle_blob(&files, "c", Ok::<_, String>(Blob::TooLarge));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "three"), 1, "{rows:?}");

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.take_request().as_deref(), Some("d"));
        s.handle_blob(&files, "zzz", text(content));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "three"), 1, "unknown path discarded: {rows:?}");
        s.handle_blob(&files, "d", text(content));
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        s.handle_key(&files, KeyModifiers::CONTROL, KeyCode::Char('f'));
        let (rows, _) = draw(&mut s, &files, 12);
        assert_eq!(count(&rows, "one"), 2, "full file unfolded: {rows:?}");
        assert_eq!(count(&rows, "five"), 2, "{rows:?}");
    }

    #[test]
    /// TU-E-032 — a hostile or truncated patch renders without a crash.
    fn ut_garbage_patch_renders() {
        let files = vec![file(
            "a",
            FileStatus::Modified,
            Some("@@ nonsense @@\n+x\nno prefix\n@@ -1,999"),
        )];
        let mut s = FilesState::new(&files);
        s.take_request();
        draw(&mut s, &files, 8);
    }

    #[test]
    /// TU-E-033 — with no files the tree is empty, the diff panel stays empty, nothing is requested and keys do nothing.
    fn ut_no_files() {
        let mut s = FilesState::new(&[]);
        assert_eq!(s.take_request(), None);
        for code in [
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Enter,
            KeyCode::Tab,
        ] {
            s.handle_key(&[], KeyModifiers::NONE, code);
        }
        s.handle_key(&[], KeyModifiers::NONE, KeyCode::Char('j'));
        s.handle_blob(&[], "x", text("x"));
        let (rows, _) = draw(&mut s, &[], 8);
        assert!(rows[0].contains(" Files "), "{rows:?}");
        assert!(
            !rows.iter().any(|l| l.contains("No diff")),
            "diff panel empty: {rows:?}"
        );
    }

    #[test]
    /// TU-R-082, TU-R-081, TU-E-052, TU-E-053 — outside review mode `c` on the focused diff notices `no review: run :review`; in review mode `c` opens the comment panel under the diff with an editable draft editor on the selected rows, marks the lines and takes the focus; Tab cycles tree → diff → comment → tree while the panel is shown; Esc on the panel returns the focus to the diff; `c` on a notice panel reads `no line to comment`.
    fn ut_comment_flow() {
        let files = vec![file(
            "a.rs",
            FileStatus::Modified,
            Some("@@ -1,3 +1,3 @@\n fn main() {\n-    old();\n+    new();\n }\n"),
        )];
        let mut s = FilesState::with_review(&files, &[]);
        s.take_request();
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('c')));
        assert_eq!(
            s.review().and_then(|r| r.notice()),
            Some("no review: run :review"),
            "TU-E-052"
        );
        assert_eq!(s.focus(), Panel::Diff);

        s.review_mut().expect("review panel").active = true;
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('c')));
        assert_eq!(s.focus(), Panel::Comment, "the editor takes the focus");
        let (rows, _) = draw(&mut s, &files, 20);
        let top = rows
            .iter()
            .position(|r| r.contains(" comment "))
            .expect("editor panel under the diff");
        assert!(
            20 - top >= 8,
            "the editor box has at least eight rows: top {top}"
        );
        for key in [
            KeyCode::Char('i'),
            KeyCode::Char('h'),
            KeyCode::Char('i'),
            KeyCode::Esc,
        ] {
            s.handle_key(&files, KeyModifiers::NONE, key);
        }
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Tree, "comment → tree");
        let review = s.review().expect("review panel");
        assert_eq!(review.pending().0[0].body, "hi", "saved on unfocus");
        assert_eq!(review.marks("a.rs").len(), 1, "gutter mark");
        assert_eq!(review.marks("a.rs")[0].color, theme::TEMPLATE.review);

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Diff);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Comment, "shown thread keeps the stop");
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Esc));
        assert_eq!(s.focus(), Panel::Diff, "Esc back to the diff");

        let none = vec![file("b.rs", FileStatus::Modified, None)];
        let mut s = FilesState::with_review(&none, &[]);
        s.review_mut().expect("review panel").active = true;
        s.handle_key(&none, KeyModifiers::NONE, KeyCode::Tab);
        assert!(s.handle_key(&none, KeyModifiers::NONE, KeyCode::Char('c')));
        assert_eq!(
            s.review().and_then(|r| r.notice()),
            Some("no line to comment"),
            "TU-E-053"
        );
    }

    #[test]
    /// TU-R-082, TU-E-053 — `c` on a visual range of added lines drafts on the new side with the file's line numbers; on removed-only lines it drafts on the old side; on a hunk header alone it notices `no line to comment`.
    fn ut_comment_side_from_selection() {
        let files = vec![file(
            "a.rs",
            FileStatus::Modified,
            Some("@@ -1,3 +1,4 @@\n one\n+alpha\n+beta\n-gone\n two\n"),
        )];
        let mut s = FilesState::with_review(&files, &[]);
        s.take_request();
        s.review_mut().expect("review").active = true;
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('c')));
        assert_eq!(
            s.review().and_then(|r| r.notice()),
            Some("no line to comment"),
            "hunk header has no line"
        );
        for key in ['j', 'j', 'v', 'j'] {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char(key));
        }
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('c')));
        assert_eq!(
            s.focus(),
            Panel::Comment,
            "{:?}",
            s.review().and_then(|r| r.notice())
        );
        for key in [KeyCode::Char('i'), KeyCode::Char('x'), KeyCode::Esc] {
            s.handle_key(&files, KeyModifiers::NONE, key);
        }
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        let added = &s.review().expect("review").pending().0[0];
        assert_eq!(
            (added.side, added.start_line, added.line),
            (crate::github::review::Side::Right, 2, 3),
            "added lines draft on the new side"
        );

        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        for key in ['j', 'c'] {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char(key));
        }
        assert_eq!(
            s.focus(),
            Panel::Comment,
            "{:?}",
            s.review().and_then(|r| r.notice())
        );
        for key in [KeyCode::Char('i'), KeyCode::Char('y'), KeyCode::Esc] {
            s.handle_key(&files, KeyModifiers::NONE, key);
        }
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        let removed = &s.review().expect("review").pending().0[1];
        assert_eq!(
            (removed.side, removed.start_line, removed.line),
            (crate::github::review::Side::Left, 2, 2),
            "a removed-only line drafts on the old side"
        );
    }

    #[test]
    /// TU-R-075 — `w` on the focused diff panel toggles the widget's line wrap, off by default; the choice is the tab's and persists across file selections; `w` on the focused tree does not toggle.
    fn ut_wrap_toggle() {
        let long = "@@ -1,1 +1,2 @@\n keep\n+alpha beta gamma delta epsilon zeta eta theta iota kappa omega\n";
        let files = vec![
            file("a.rs", FileStatus::Modified, Some(long)),
            file("b.rs", FileStatus::Modified, Some(long)),
        ];
        let mut s = FilesState::new(&files);
        s.take_request();
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('w'));
        let (rows, _) = draw(&mut s, &files, 12);
        assert!(
            !rows.iter().any(|r| r.contains("omega")),
            "tree w is no toggle, the long line stays clipped: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert!(s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('w')));
        let (rows, _) = draw(&mut s, &files, 12);
        assert!(
            rows.iter().any(|r| r.contains("omega")),
            "wrapped tail visible: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(s.selected(), Some(1));
        s.take_request();
        let (rows, _) = draw(&mut s, &files, 12);
        assert!(
            rows.iter().any(|r| r.contains("omega")),
            "wrap persists across files: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('w'));
        let (rows, _) = draw(&mut s, &files, 12);
        assert!(
            !rows.iter().any(|r| r.contains("omega")),
            "back to clipped: {rows:?}"
        );
    }

    #[test]
    /// TU-R-081, TU-R-083 — with a remote thread's line selected the panel shows the thread box; `r` opens the reply box under it and Tab cycles tree → diff → thread box → reply box → tree; the panel grows to half the tab height while the reply box is open.
    fn ut_thread_and_reply_focus_cycle() {
        let thread = ReviewThread {
            id: "T_1".into(),
            path: "a.rs".into(),
            side: crate::github::review::Side::Right,
            start_line: 2,
            line: 2,
            resolved: false,
            outdated: false,
            comments: vec![crate::github::pull::ThreadComment {
                author: Some("octo".into()),
                body: "why?".into(),
            }],
        };
        let files = vec![file(
            "a.rs",
            FileStatus::Modified,
            Some("@@ -1,3 +1,3 @@\n fn main() {\n-    old();\n+    new();\n }\n"),
        )];
        let mut s = FilesState::with_review(&files, &[thread]);
        s.take_request();
        s.review_mut().expect("review").active = true;
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        for _ in 0..2 {
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j'));
        }
        let (rows, _) = draw(&mut s, &files, 24);
        assert!(
            rows.iter().any(|r| r.contains(" thread "))
                && rows.iter().any(|r| r.contains(" @octo ")),
            "thread box shown on the marked line: {rows:?}"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Comment);
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('r'));
        let (rows, _) = draw(&mut s, &files, 24);
        assert!(
            rows.iter().any(|r| r.contains(" thread "))
                && rows.iter().any(|r| r.contains(" reply ")),
            "thread box stays over the reply box: {rows:?}"
        );
        let panel_top = rows
            .iter()
            .position(|r| r.contains(" thread "))
            .expect("top");
        assert!(
            (24 - panel_top as u16) >= 12,
            "panel grew to half the tab: top {panel_top}"
        );
        for key in [
            KeyCode::Char('i'),
            KeyCode::Char('o'),
            KeyCode::Char('k'),
            KeyCode::Esc,
        ] {
            s.handle_key(&files, KeyModifiers::NONE, key);
        }
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::BackTab);
        assert_eq!(s.focus(), Panel::Comment, "reply back to the thread box");
        assert!(
            s.handle_key(&files, KeyModifiers::NONE, KeyCode::Char('j')),
            "thread box consumes scroll keys"
        );
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Comment, "thread box forward to the reply");
        s.handle_key(&files, KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(s.focus(), Panel::Tree, "reply forward leaves the panel");
        assert_eq!(
            s.review().expect("review").pending().1[0].body,
            "ok",
            "leaving the panel saved the reply"
        );
    }
}
