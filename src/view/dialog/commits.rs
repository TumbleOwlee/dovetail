//! The `Commits` tab of the pull request overlay.

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::Border;
use ferrowl_ui::state::{TableState, TableStateBuilder};
use ferrowl_ui::style::TableStyle;
use ferrowl_ui::traits::HandleEvents;
use ferrowl_ui::widgets::{Header, Table, TableBuilder, TableEntry, Widget, Width};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::widgets::StatefulWidget;

use crate::github::pull::{Commit, CommitAuthor};
use crate::view::theme;

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
    table: Widget<TableState<CommitRow, COLUMNS>, Table<CommitRow, CommitHeader, COLUMNS>>,
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
            table: Widget {
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
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> Option<usize> {
        self.table.state.table_state().selected()
    }

    pub fn handle_key(&mut self, modifiers: KeyModifiers, code: KeyCode) {
        self.table.state.handle_events(modifiers, code);
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, theme::base());
        StatefulWidget::render(&self.table.widget, area, buf, &mut self.table.state);
    }
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
}
