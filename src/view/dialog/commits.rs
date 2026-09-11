//! The `Commits` tab of the pull request overlay: the commit table, and a selected
//! commit's changes in the `Files Changed` layout.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::Border;
use ferrowl_ui::state::{TableState, TableStateBuilder};
use ferrowl_ui::style::TableStyle;
use ferrowl_ui::traits::HandleEvents;
use ferrowl_ui::widgets::{Header, Table, TableBuilder, TableEntry, Widget as TableWidget, Width};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, StatefulWidget, Widget};

use crate::github::blob::Blob;
use crate::github::files::ChangedFile;
use crate::github::pull::{Commit, CommitAuthor};
use crate::view::dialog::files::FilesState;
use crate::view::theme;

const LOADING: &str = "Loading commit..";

/// A request the caller queues for a commit diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitRequest {
    /// The commit's changed files.
    Files { sha: String },
    /// A file's content at the commit.
    Blob { sha: String, path: String },
}

/// What is known about one commit's changes.
enum CommitDiff {
    Loading,
    Failed(String),
    Loaded {
        files: Vec<ChangedFile>,
        state: Box<FilesState>,
    },
}

const COLUMNS: usize = 4;

#[derive(Debug, Clone)]
pub struct CommitRow(pub Commit);

#[derive(Debug, Clone)]
pub struct CommitHeader;

impl Header<COLUMNS> for CommitHeader {
    fn header() -> [String; COLUMNS] {
        ["Commit ID", "Description", "Author", "Date"].map(String::from)
    }

    fn widths() -> [Width; COLUMNS] {
        [
            Width { min: 9, max: 12 },
            Width { min: 20, max: 160 },
            Width { min: 8, max: 32 },
            Width { min: 10, max: 10 },
        ]
    }
}

impl TableEntry<COLUMNS> for CommitRow {
    fn values(&self) -> [String; COLUMNS] {
        let commit = &self.0;
        let author = match &commit.author {
            CommitAuthor::User(login) => format!("@{login}"),
            CommitAuthor::Git(name) => name.clone(),
        };
        [
            commit.sha.clone(),
            commit.headline.clone(),
            author,
            commit.date.chars().take(10).collect(),
        ]
    }

    fn height(&self) -> u16 {
        1
    }

    fn cell_styles(&self) -> [Option<Style>; COLUMNS] {
        [None; COLUMNS]
    }
}

pub struct CommitsView {
    table: TableWidget<TableState<CommitRow, COLUMNS>, Table<CommitRow, CommitHeader, COLUMNS>>,
    commits: Vec<Commit>,
    /// What is known per commit id, kept while the overlay is open.
    diffs: HashMap<String, CommitDiff>,
    /// The commit whose changes show in place of the table.
    open: Option<String>,
    /// A request the caller has yet to queue.
    request: Option<CommitRequest>,
}

impl CommitsView {
    /// Rows in the given order, the first selected.
    pub fn new(commits: &[Commit]) -> CommitsView {
        let style = TableStyle {
            focused: theme::on_bg(theme::TEMPLATE.hi),
            border: theme::on_bg(theme::TEMPLATE.hi),
            general: theme::on_bg(theme::TEMPLATE.border),
            ..TableStyle::default()
        };
        CommitsView {
            table: TableWidget {
                state: TableStateBuilder::default()
                    .values(commits.iter().cloned().map(CommitRow).collect())
                    .build()
                    .expect("values are set"),
                widget: TableBuilder::default()
                    .border(Border::Full(Margin::new(1, 0)))
                    .title(Some("Commits".into()))
                    .style(style)
                    .split_by_whitespace([false; COLUMNS])
                    .build()
                    .expect("TableBuilder fields all default"),
            },
            commits: commits.to_vec(),
            diffs: HashMap::new(),
            open: None,
            request: None,
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> Option<usize> {
        self.table.state.table_state().selected()
    }

    /// Whether a commit's changes show in place of the table.
    pub fn diff_open(&self) -> bool {
        self.open.is_some()
    }

    /// The request the caller must queue, once.
    pub fn take_request(&mut self) -> Option<CommitRequest> {
        self.request.take()
    }

    /// Routes one key; reports whether the tab consumed it, so an unconsumed Esc can
    /// close the overlay.
    pub fn handle_key(&mut self, modifiers: KeyModifiers, code: KeyCode) -> bool {
        let Some(sha) = self.open.clone() else {
            if code == KeyCode::Enter {
                self.open_selected();
                return true;
            }
            self.table.state.handle_events(modifiers, code);
            return true;
        };
        match self.diffs.get_mut(&sha) {
            Some(CommitDiff::Loaded { files, state }) => {
                let consumed = state.handle_key(files, modifiers, code);
                if let Some(path) = state.take_request() {
                    self.request = Some(CommitRequest::Blob { sha, path });
                }
                if consumed {
                    return true;
                }
            }
            _ => {
                if code != KeyCode::Esc {
                    return false;
                }
            }
        }
        if code == KeyCode::Esc {
            self.open = None;
            return true;
        }
        false
    }

    /// Opens the selected commit's diff, requesting its files unless already loading or
    /// loaded; a failed earlier request is retried.
    fn open_selected(&mut self) {
        let Some(commit) = self
            .table
            .state
            .table_state()
            .selected()
            .and_then(|i| self.commits.get(i))
        else {
            return;
        };
        let sha = commit.sha.clone();
        match self.diffs.get(&sha) {
            None | Some(CommitDiff::Failed(_)) => {
                self.diffs.insert(sha.clone(), CommitDiff::Loading);
                self.request = Some(CommitRequest::Files { sha: sha.clone() });
            }
            Some(CommitDiff::Loading | CommitDiff::Loaded { .. }) => {}
        }
        self.open = Some(sha);
    }

    /// Stores a commit's file list; ignored for a commit never requested.
    pub fn handle_commit(&mut self, sha: &str, result: Result<Vec<ChangedFile>, impl ToString>) {
        let Some(entry) = self.diffs.get_mut(sha) else {
            return;
        };
        *entry = match result {
            Ok(files) => {
                let mut state = Box::new(FilesState::new(&files));
                if let Some(path) = state.take_request() {
                    self.request = Some(CommitRequest::Blob {
                        sha: sha.to_string(),
                        path,
                    });
                }
                CommitDiff::Loaded { files, state }
            }
            Err(e) => CommitDiff::Failed(e.to_string()),
        };
    }

    /// Stores a file's content at `oid`; ignored for a commit never requested.
    pub fn handle_blob(&mut self, oid: &str, path: &str, result: Result<Blob, impl ToString>) {
        let Some(CommitDiff::Loaded { files, state }) = self.diffs.get_mut(oid) else {
            return;
        };
        state.handle_blob(files, path, result);
        if let Some(path) = state.take_request() {
            self.request = Some(CommitRequest::Blob {
                sha: oid.to_string(),
                path,
            });
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, theme::base());
        let Some(sha) = self.open.clone() else {
            StatefulWidget::render(&self.table.widget, area, buf, &mut self.table.state);
            return;
        };
        match self.diffs.get_mut(&sha) {
            Some(CommitDiff::Loaded { files, state }) => state.render(files, area, buf),
            Some(CommitDiff::Failed(message)) => {
                notice(&sha, message, theme::TEMPLATE.error, area, buf);
            }
            _ => notice(&sha, LOADING, theme::TEMPLATE.placeholder, area, buf),
        }
    }
}

/// A bordered panel titled by the commit id holding one line of `text`.
fn notice(sha: &str, text: &str, color: ratatui::style::Color, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .style(theme::on_bg(theme::TEMPLATE.hi))
        .title(format!(" {sha} "));
    let inner = block.inner(area).inner(Margin::new(1, 0));
    block.render(area, buf);
    Paragraph::new(Line::styled(text.to_string(), theme::on_bg(color)))
        .style(theme::base())
        .render(inner, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{buffer_rows, render_buffer};
    use crate::view::theme;

    fn commit(i: u32, author: CommitAuthor) -> Commit {
        Commit {
            sha: format!("sha{i:04}"),
            headline: format!("Change {i}"),
            author,
            date: format!("2026-09-{i:02}T10:00:00Z"),
        }
    }

    #[test]
    /// TU-R-073 — a table titled `Commits` with the four columns, `@login` for a linked author and the git name otherwise, the date as `YYYY-MM-DD`; the first row is selected and the table's keys move the selection; the border is in the highlight color.
    fn ut_table_columns_and_selection() {
        let commits = vec![
            commit(1, CommitAuthor::User("octo".into())),
            commit(2, CommitAuthor::Git("Anon Y".into())),
            commit(3, CommitAuthor::User("a".into())),
        ];
        let mut v = CommitsView::new(&commits);
        assert_eq!(v.selected(), Some(0));
        let buf = render_buffer(80, 8, |f| v.render(f.area(), f.buffer_mut()));
        let rows = buffer_rows(&buf);
        assert!(rows[0].contains("Commits"), "{rows:?}");
        assert!(
            rows[1].contains("Commit ID")
                && rows[1].contains("Description")
                && rows[1].contains("Author")
                && rows[1].contains("Date"),
            "{rows:?}"
        );
        let first = rows
            .iter()
            .find(|r| r.contains("sha0001"))
            .expect("first row");
        assert!(
            first.contains("Change 1") && first.contains("@octo") && first.contains("2026-09-01"),
            "{first}"
        );
        assert!(!first.contains("T10:00"), "{first}");
        let second = rows
            .iter()
            .find(|r| r.contains("sha0002"))
            .expect("second row");
        assert!(
            second.contains("Anon Y") && !second.contains("@Anon"),
            "{second}"
        );
        assert_eq!(buf[(0, 0)].fg, theme::TEMPLATE.hi, "border highlighted");
        v.handle_key(KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(v.selected(), Some(1));
        v.handle_key(KeyModifiers::NONE, KeyCode::Down);
        assert_eq!(v.selected(), Some(2));
        v.handle_key(KeyModifiers::NONE, KeyCode::Char('k'));
        assert_eq!(v.selected(), Some(1));
        v.handle_key(KeyModifiers::NONE, KeyCode::Char('g'));
        assert_eq!(v.selected(), Some(0));
        v.handle_key(KeyModifiers::SHIFT, KeyCode::Char('G'));
        assert_eq!(v.selected(), Some(2));
    }

    #[test]
    /// TU-R-073 — with no commits the table shows only its header and keys do nothing harmful.
    fn ut_empty() {
        let mut v = CommitsView::new(&[]);
        v.handle_key(KeyModifiers::NONE, KeyCode::Char('j'));
        let rows = buffer_rows(&render_buffer(60, 6, |f| {
            v.render(f.area(), f.buffer_mut())
        }));
        assert!(rows[1].contains("Commit ID"), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("sha")), "{rows:?}");
    }

    fn changed(path: &str, patch: &str) -> crate::github::files::ChangedFile {
        crate::github::files::ChangedFile {
            path: path.into(),
            previous_path: None,
            status: crate::github::files::FileStatus::Modified,
            additions: 1,
            deletions: 1,
            patch: Some(patch.into()),
        }
    }

    fn draw(v: &mut CommitsView) -> Vec<String> {
        buffer_rows(&render_buffer(100, 10, |f| {
            v.render(f.area(), f.buffer_mut())
        }))
    }

    #[test]
    /// TU-R-078, TU-E-047, TU-E-050 — Enter opens the selected commit's diff in place of the table: its file list is requested once, `Loading commit..` in a panel titled by the commit id while it runs, the error message after a failure; a list for a commit never requested is discarded; Enter after a failure requests again; the arrived list shows in the `Files Changed` layout and the first file's content is requested at the commit; Esc returns to the table and reopening the commit keeps the loaded state without a new request.
    fn ut_commit_diff_lifecycle() {
        let commits = vec![
            commit(1, CommitAuthor::User("octo".into())),
            commit(2, CommitAuthor::Git("Anon Y".into())),
        ];
        let mut v = CommitsView::new(&commits);
        assert!(v.handle_key(KeyModifiers::NONE, KeyCode::Enter));
        assert!(v.diff_open());
        assert_eq!(
            v.take_request(),
            Some(CommitRequest::Files {
                sha: "sha0001".into()
            })
        );
        assert_eq!(v.take_request(), None, "requested once");
        let rows = draw(&mut v);
        assert!(rows[0].contains(" sha0001 "), "{rows:?}");
        assert!(rows[1].contains(LOADING), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("Commit ID")), "{rows:?}");

        v.handle_commit("zzz", Ok::<_, String>(vec![]));
        let rows = draw(&mut v);
        assert!(
            rows[1].contains(LOADING),
            "unknown commit discarded: {rows:?}"
        );

        v.handle_commit("sha0001", Err::<Vec<_>, _>("github: HTTP 500"));
        let rows = draw(&mut v);
        assert!(rows[1].contains("github: HTTP 500"), "{rows:?}");
        assert_eq!(v.take_request(), None);

        assert!(v.handle_key(KeyModifiers::NONE, KeyCode::Esc));
        assert!(!v.diff_open());
        let rows = draw(&mut v);
        assert!(rows[1].contains("Commit ID"), "table is back: {rows:?}");

        v.handle_key(KeyModifiers::NONE, KeyCode::Enter);
        assert_eq!(
            v.take_request(),
            Some(CommitRequest::Files {
                sha: "sha0001".into()
            }),
            "a failed list is requested again"
        );
        let rows = draw(&mut v);
        assert!(rows[1].contains(LOADING), "{rows:?}");

        v.handle_commit(
            "sha0001",
            Ok::<_, String>(vec![changed("a.rs", "@@ -1 +1 @@\n-x\n+y\n")]),
        );
        assert_eq!(
            v.take_request(),
            Some(CommitRequest::Blob {
                sha: "sha0001".into(),
                path: "a.rs".into()
            })
        );
        let rows = draw(&mut v);
        assert!(
            rows[0].contains(" Files ") && rows[0].contains(" a.rs "),
            "{rows:?}"
        );
        v.handle_blob(
            "sha0001",
            "a.rs",
            Ok::<_, String>(crate::github::blob::Blob::Text("y\n".into())),
        );
        assert_eq!(v.take_request(), None);
        v.handle_blob(
            "zzz",
            "a.rs",
            Ok::<_, String>(crate::github::blob::Blob::Text("y\n".into())),
        );

        assert!(
            v.handle_key(KeyModifiers::NONE, KeyCode::Tab),
            "keys reach the files layout"
        );
        assert!(!v.handle_key(KeyModifiers::NONE, KeyCode::Char('q')));
        assert!(v.handle_key(KeyModifiers::NONE, KeyCode::Esc));
        assert!(!v.diff_open());

        v.handle_key(KeyModifiers::NONE, KeyCode::Enter);
        assert_eq!(v.take_request(), None, "loaded state kept on reopen");
        let rows = draw(&mut v);
        assert!(rows[0].contains(" a.rs "), "{rows:?}");
        v.handle_key(KeyModifiers::NONE, KeyCode::Esc);

        v.handle_key(KeyModifiers::NONE, KeyCode::Char('j'));
        v.handle_key(KeyModifiers::NONE, KeyCode::Enter);
        assert_eq!(
            v.take_request(),
            Some(CommitRequest::Files {
                sha: "sha0002".into()
            }),
            "each commit requested on its own"
        );
    }

    #[test]
    /// TU-E-048 — Enter on the commit table with no commits does nothing.
    fn ut_enter_with_no_commits() {
        let mut v = CommitsView::new(&[]);
        assert!(v.handle_key(KeyModifiers::NONE, KeyCode::Enter));
        assert!(!v.diff_open());
        assert_eq!(v.take_request(), None);
    }
}
