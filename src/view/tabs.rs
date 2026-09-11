//! The tab line and the read-only summary each tab shows.

use ferrowl_ui::state::TabBarState;
use ferrowl_ui::widgets::TabBarBuilder;
use ratatui::buffer::Buffer;
use ratatui::layout::{Direction, Margin, Rect};
use ratatui::widgets::StatefulWidget;

use crate::view::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Board,
    Remote,
}

impl Tab {
    pub const ALL: [Tab; 2] = [Tab::Board, Tab::Remote];

    /// Wraps at both ends.
    pub fn next(self) -> Tab {
        let i = Tab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Tab::ALL[(i + 1) % Tab::ALL.len()]
    }

    /// Wraps at both ends.
    pub fn previous(self) -> Tab {
        let i = Tab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Tab::ALL[(i + Tab::ALL.len() - 1) % Tab::ALL.len()]
    }

    /// Zero-based position in the tab line.
    pub fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    /// The tab line's caption, written down the line.
    pub fn label(self) -> &'static str {
        match self {
            Tab::Board => "BOARD",
            Tab::Remote => "REPOSITORY",
        }
    }
}

/// Columns the tab line takes: the character column and one blank column each side.
pub const TAB_LINE_WIDTH: u16 = 3;

pub fn render_tab_line(area: Rect, buf: &mut Buffer, active: Tab) {
    render_vertical_tabs(
        area,
        buf,
        Tab::ALL.iter().map(|t| t.label().to_string()).collect(),
        active.index(),
    );
}

/// The captions stacked down `area` in the scheme's tab style, `active` selected.
pub fn render_vertical_tabs(area: Rect, buf: &mut Buffer, titles: Vec<String>, active: usize) {
    let mut state = TabBarState {
        titles,
        active,
        offset: 0,
    };
    let tabs = TabBarBuilder::<String>::default()
        .style(theme::tab_bar_style())
        .padding(Margin::new(1, 1))
        .direction(Direction::Vertical)
        .build()
        .expect("TabBarBuilder fields all default");
    StatefulWidget::render(&tabs, area, buf, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// TU-R-019 — the captions are `BOARD` and `REPOSITORY`.
    fn ut_tab_labels() {
        assert_eq!(Tab::Board.label(), "BOARD");
        assert_eq!(Tab::Remote.label(), "REPOSITORY");
    }

    #[test]
    /// TU-R-019, TU-E-044 — the tab line stacks both labels one character per row in the middle column with blank columns beside, the active one in the selected style; a short area scrolls to the active tab.
    fn ut_tab_line_stacks_labels() {
        let buf = crate::testkit::render_buffer(TAB_LINE_WIDTH, 40, |f| {
            render_tab_line(f.area(), f.buffer_mut(), Tab::Remote);
        });
        let column = crate::testkit::buffer_column(&buf, 1);
        let board = column.find("BOARD").expect("board label");
        let remote = column.find("REPOSITORY").expect("remote label");
        assert!(board < remote, "{column:?}");
        assert!(
            crate::testkit::buffer_column(&buf, 0).is_empty()
                && crate::testkit::buffer_column(&buf, 2).is_empty(),
            "blank side columns"
        );
        let style = theme::tab_bar_style();
        assert_eq!(
            buf[(1, board as u16)].fg,
            style.general.fg.expect("general fg")
        );
        assert_eq!(
            buf[(1, remote as u16)].fg,
            style.selected.fg.expect("selected fg")
        );
        assert_ne!(
            buf[(1, board as u16)].bg,
            buf[(1, remote as u16)].bg,
            "active block stands out"
        );
        let buf = crate::testkit::render_buffer(TAB_LINE_WIDTH, 8, |f| {
            render_tab_line(f.area(), f.buffer_mut(), Tab::Remote);
        });
        let column = crate::testkit::buffer_column(&buf, 1);
        assert!(
            column.contains("REPOS") || column.contains("SITORY"),
            "scrolled to the active tab: {column:?}"
        );
    }
}
