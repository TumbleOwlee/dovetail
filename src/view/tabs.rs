//! The tab line and the read-only summary each tab shows.

use ferrowl_ui::state::VerticalTabsState;
use ferrowl_ui::widgets::VerticalTabsBuilder;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, StatefulWidget, Widget};

use crate::config::{Section, Settings};
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

    pub fn section(self, settings: &Settings) -> &dyn Section {
        match self {
            Tab::Board => &settings.board,
            Tab::Remote => &settings.remote,
        }
    }
}

/// The lines a tab body shows.
pub fn summary_lines(
    tab: Tab,
    settings: Option<&Settings>,
    credentials_present: bool,
) -> Vec<String> {
    let Some(settings) = settings else {
        return vec!["Repository not configured. Run :config to set it up.".to_string()];
    };
    let section = tab.section(settings);
    let mut lines = vec![format!("kind: {}", section.kind())];
    lines.extend(
        section
            .identifiers()
            .into_iter()
            .map(|(key, value)| format!("{key}: {value}")),
    );
    lines.push(format!(
        "profile: {}",
        section.credentials().unwrap_or("none")
    ));
    lines.push(format!(
        "credentials: {}",
        if credentials_present {
            "present"
        } else {
            "missing"
        }
    ));
    lines
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
    let mut state = VerticalTabsState {
        titles,
        active,
        offset: 0,
    };
    let tabs = VerticalTabsBuilder::<String>::default()
        .style(theme::scrolling_tabs_style())
        .padding(Margin::new(1, 1))
        .build()
        .expect("VerticalTabsBuilder fields all default");
    StatefulWidget::render(&tabs, area, buf, &mut state);
}

pub fn render_body(area: Rect, buf: &mut Buffer, lines: &[String]) {
    let text = Text::from(
        lines
            .iter()
            .map(|l| Line::from(l.as_str()))
            .collect::<Vec<_>>(),
    );
    Paragraph::new(text)
        .style(Style::default().fg(theme::TEMPLATE.text).bg(theme::BG))
        .render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Board, Remote, Source};

    fn settings() -> Settings {
        Settings {
            board: Board::Jira {
                credentials: Some("j".into()),
                project_key: "ACME".into(),
            },
            remote: Remote::Github {
                credentials: None,
                owner: "o".into(),
                repo: "r".into(),
            },
            source: Source::UserFile,
        }
    }

    #[test]
    /// TU-R-020 — next and previous wrap at both ends.
    fn ut_tab_next_previous_wrap() {
        assert_eq!(Tab::Board.next(), Tab::Remote);
        assert_eq!(Tab::Remote.next(), Tab::Board);
        assert_eq!(Tab::Board.previous(), Tab::Remote);
        assert_eq!(Tab::Remote.previous(), Tab::Board);
    }

    #[test]
    /// TU-R-019 — the captions are `BOARD` and `REPOSITORY`.
    fn ut_tab_labels() {
        assert_eq!(Tab::Board.label(), "BOARD");
        assert_eq!(Tab::Remote.label(), "REPOSITORY");
    }

    #[test]
    /// TU-R-022 — the board summary lists kind, identifiers, profile and credential presence.
    fn ut_board_summary_lines() {
        let s = settings();
        assert_eq!(
            summary_lines(Tab::Board, Some(&s), true),
            vec![
                "kind: jira",
                "project_key: ACME",
                "profile: j",
                "credentials: present"
            ]
        );
    }

    #[test]
    /// TU-R-023 — the remote summary shows `none` and `missing` without a profile.
    fn ut_remote_summary_lines() {
        let s = settings();
        assert_eq!(
            summary_lines(Tab::Remote, Some(&s), false),
            vec![
                "kind: github",
                "owner: o",
                "repo: r",
                "profile: none",
                "credentials: missing"
            ]
        );
    }

    #[test]
    /// TU-R-024 — without settings the body is one line naming the `config` command.
    fn ut_unconfigured_summary_is_single_line() {
        let lines = summary_lines(Tab::Board, None, false);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains("not configured") && lines[0].contains(":config"),
            "{lines:?}"
        );
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
        let style = theme::scrolling_tabs_style();
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

    #[test]
    /// TU-R-022 — the body renders one summary line per row.
    fn ut_body_renders_lines() {
        let rows = crate::testkit::render_rows(30, 3, |f| {
            render_body(
                f.area(),
                f.buffer_mut(),
                &["a: 1".to_string(), "b: 2".to_string()],
            );
        });
        assert_eq!(rows[0], "a: 1");
        assert_eq!(rows[1], "b: 2");
    }
}
