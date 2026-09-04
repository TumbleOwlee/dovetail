//! The tab line and the read-only summary each tab shows.

use ferrowl_ui::COLOR_SCHEME;
use ferrowl_ui::state::ScrollingTabsState;
use ferrowl_ui::widgets::ScrollingTabsBuilder;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, StatefulWidget, Widget};

use crate::config::{Kind, Section, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Board,
    Remote,
}

impl Tab {
    pub const ALL: [Tab; 2] = [Tab::Board, Tab::Remote];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Board => "Task Board",
            Tab::Remote => "Git Remote",
        }
    }

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

    /// `<title> [<Kind>]`, `[-]` without settings.
    pub fn label(self, settings: Option<&Settings>) -> String {
        let kind = settings.map_or("-", |s| kind_label(self.section(s).kind()));
        format!("{} [{kind}]", self.title())
    }

    pub fn section(self, settings: &Settings) -> &dyn Section {
        match self {
            Tab::Board => &settings.board,
            Tab::Remote => &settings.remote,
        }
    }
}

/// Capitalised kind name for labels.
pub fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Github => "GitHub",
        Kind::Jira => "Jira",
        Kind::Bitbucket => "Bitbucket",
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

pub fn render_tab_line(area: Rect, buf: &mut Buffer, active: Tab, settings: Option<&Settings>) {
    let mut state = ScrollingTabsState {
        titles: Tab::ALL
            .iter()
            .map(|t| t.label(settings))
            .collect::<Vec<String>>(),
        selected: Tab::ALL.iter().position(|t| *t == active).unwrap_or(0),
    };
    let tabs = ScrollingTabsBuilder::<String>::default()
        .build()
        .expect("ScrollingTabsBuilder fields all default");
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
        .style(Style::default().fg(COLOR_SCHEME.text).bg(COLOR_SCHEME.bg))
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
    /// TU-R-019 — labels carry the configured kind in brackets, `[-]` without settings.
    fn ut_tab_labels_show_kind() {
        let s = settings();
        assert_eq!(Tab::Board.label(Some(&s)), "Task Board [Jira]");
        assert_eq!(Tab::Remote.label(Some(&s)), "Git Remote [GitHub]");
        assert_eq!(Tab::Board.label(None), "Task Board [-]");
        assert_eq!(Tab::Remote.label(None), "Git Remote [-]");
        assert_eq!(kind_label(Kind::Bitbucket), "Bitbucket");
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
    /// TU-R-019 — the tab line renders both labels in order.
    fn ut_tab_line_renders_both_labels() {
        let s = settings();
        let rows = crate::testkit::render_rows(60, 1, |f| {
            render_tab_line(f.area(), f.buffer_mut(), Tab::Remote, Some(&s));
        });
        let board = rows[0].find("Task Board [Jira]").expect("board label");
        let remote = rows[0].find("Git Remote [GitHub]").expect("remote label");
        assert!(board < remote);
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
