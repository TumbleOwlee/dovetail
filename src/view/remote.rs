//! Git Remote tab body: the pull request table.

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::state::{TableState, TableStateBuilder};
use ferrowl_ui::style::TableStyle;
use ferrowl_ui::traits::HandleEvents;
use ferrowl_ui::widgets::{Header, Table, TableBuilder, TableEntry, Widget, Width};
use ferrowl_ui::{Border, COLOR_SCHEME};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::widgets::StatefulWidget;

use crate::github::pulls::{PullRequest, PullState};
use crate::view::theme;

const COLUMNS: usize = 6;

#[derive(Debug, Clone, Default)]
pub struct PullRow(pub PullRequest);

#[derive(Debug, Clone)]
pub struct PullHeader;

impl Header<COLUMNS> for PullHeader {
    fn header() -> [String; COLUMNS] {
        ["#", "Title", "State", "Author", "Branch", "Updated"].map(String::from)
    }

    fn widths() -> [Width; COLUMNS] {
        [
            Width { min: 4, max: 7 },
            Width { min: 20, max: 120 },
            Width { min: 6, max: 6 },
            Width { min: 8, max: 24 },
            Width { min: 12, max: 60 },
            Width { min: 10, max: 10 },
        ]
    }
}

impl TableEntry<COLUMNS> for PullRow {
    fn values(&self) -> [String; COLUMNS] {
        let pull = &self.0;
        [
            pull.number.to_string(),
            pull.title.clone(),
            state_label(pull).to_string(),
            pull.author
                .as_ref()
                .map_or_else(String::new, |a| format!("@{a}")),
            format!("{} → {}", pull.head, pull.base),
            pull.updated_at.chars().take(10).collect(),
        ]
    }

    fn height(&self) -> u16 {
        1
    }

    fn cell_styles(&self) -> [Option<Style>; COLUMNS] {
        let color = match (self.0.state, self.0.draft) {
            (PullState::Open, false) => COLOR_SCHEME.success,
            (PullState::Open, true) => COLOR_SCHEME.placeholder,
            (PullState::Merged, _) => COLOR_SCHEME.hi,
            (PullState::Closed, _) => COLOR_SCHEME.error,
        };
        [
            None,
            None,
            Some(Style::default().fg(color)),
            None,
            None,
            None,
        ]
    }
}

fn state_label(pull: &PullRequest) -> &'static str {
    match (pull.state, pull.draft) {
        (PullState::Open, false) => "open",
        (PullState::Open, true) => "draft",
        (PullState::Merged, _) => "merged",
        (PullState::Closed, _) => "closed",
    }
}

pub struct RemoteView {
    table: Widget<TableState<PullRow, COLUMNS>, Table<PullRow, PullHeader, COLUMNS>>,
}

impl RemoteView {
    /// Rows in the given order, the first selected.
    pub fn new(pulls: Vec<PullRequest>) -> RemoteView {
        let style = TableStyle {
            border: theme::on_bg(COLOR_SCHEME.border),
            general: theme::on_bg(COLOR_SCHEME.border),
            ..TableStyle::default()
        };
        RemoteView {
            table: Widget {
                state: TableStateBuilder::default()
                    .values(pulls.into_iter().map(PullRow).collect())
                    .build()
                    .expect("values are set"),
                widget: TableBuilder::default()
                    .border(Border::Full(Margin::new(1, 0)))
                    .title(Some("Pull requests".into()))
                    .style(style)
                    .split_by_whitespace([false; COLUMNS])
                    .build()
                    .expect("TableBuilder fields all default"),
            },
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> Option<&PullRequest> {
        let index = self.table.state.table_state().selected()?;
        self.table.state.values().get(index).map(|row| &row.0)
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
    use crate::github::pulls::PullState;
    use crate::testkit::render_rows;

    fn pull(number: u64, title: &str, state: PullState, draft: bool) -> PullRequest {
        PullRequest {
            number,
            title: title.into(),
            state,
            draft,
            author: Some("octo".into()),
            head: "fix".into(),
            base: "main".into(),
            updated_at: "2026-09-04T10:00:00Z".into(),
        }
    }

    fn pulls() -> Vec<PullRequest> {
        vec![
            pull(5, "Fix crash", PullState::Open, false),
            pull(3, "Draft work", PullState::Open, true),
            pull(2, "Shipped", PullState::Merged, false),
            pull(1, "Abandoned", PullState::Closed, false),
        ]
    }

    #[test]
    /// TU-R-063 — bordered table filling the body with the six columns, one row per pull request in order.
    fn ut_render_table() {
        let mut v = RemoteView::new(pulls());
        let rows = render_rows(100, 12, |f| v.render(f.area(), f.buffer_mut()));
        assert!(rows[0].starts_with('┌'), "{}", rows[0]);
        assert!(rows[11].starts_with('└'), "fills the height: {}", rows[11]);
        let header = rows.iter().find(|r| r.contains("Title")).expect("header");
        for column in ["#", "Title", "State", "Author", "Branch", "Updated"] {
            assert!(header.contains(column), "{header}");
        }
        let body: Vec<&String> = rows.iter().filter(|r| r.contains("main")).collect();
        assert_eq!(body.len(), 4, "{rows:?}");
        assert!(
            body[0].contains("5") && body[0].contains("Fix crash") && body[0].contains("open"),
            "{}",
            body[0]
        );
        assert!(
            body[0].contains("@octo")
                && body[0].contains("fix → main")
                && body[0].contains("2026-09-04"),
            "{}",
            body[0]
        );
        assert!(!body[0].contains("10:00"), "date only: {}", body[0]);
        assert!(body[1].contains("draft"), "{}", body[1]);
        assert!(body[2].contains("merged"), "{}", body[2]);
        assert!(body[3].contains("closed"), "{}", body[3]);
    }

    #[test]
    /// TU-E-023 — no pull requests: only the header.
    fn ut_empty_table() {
        let mut v = RemoteView::new(vec![]);
        let rows = render_rows(80, 6, |f| v.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains("Title")), "{rows:?}");
        assert!(v.selected().is_none());
    }

    #[test]
    /// TU-R-064 — the first row is selected; `j`/`k` move without wrapping.
    fn ut_selection_moves() {
        let mut v = RemoteView::new(pulls());
        assert_eq!(v.selected().map(|p| p.number), Some(5));
        v.handle_key(KeyModifiers::NONE, KeyCode::Char('k'));
        assert_eq!(v.selected().map(|p| p.number), Some(5));
        for _ in 0..10 {
            v.handle_key(KeyModifiers::NONE, KeyCode::Char('j'));
        }
        assert_eq!(v.selected().map(|p| p.number), Some(1));
        v.handle_key(KeyModifiers::NONE, KeyCode::Char('k'));
        assert_eq!(v.selected().map(|p| p.number), Some(2));
    }
}
