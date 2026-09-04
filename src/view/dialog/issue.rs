//! Issue details overlay.

use crossterm::event::KeyCode;
use ferrowl_ui::COLOR_SCHEME;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, HorizontalAlignment, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::github::issue::{Issue, IssueState};
use crate::view::board::{badge_text_color, label_color, wrap_title};
use crate::view::theme;

/// Screen cells left free around the overlay on each side.
const INSET: Margin = Margin::new(4, 1);

/// What the caller does after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueEvent {
    Consumed,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    Loading,
    Failed(String),
    Loaded { issue: Issue, scroll: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueDialog {
    number: u64,
    title: String,
    content: Content,
}

impl IssueDialog {
    /// Opens in the loading state for the card's issue.
    pub fn new(number: u64, title: String) -> IssueDialog {
        IssueDialog {
            number,
            title,
            content: Content::Loading,
        }
    }

    /// Replaces the loading state with the details or the failure.
    pub fn set_result(&mut self, result: Result<Issue, impl ToString>) {
        self.content = match result {
            Ok(issue) => Content::Loaded { issue, scroll: 0 },
            Err(e) => Content::Failed(e.to_string()),
        };
    }

    pub fn handle_key(&mut self, code: KeyCode) -> IssueEvent {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => return IssueEvent::Close,
            KeyCode::Char('j') => {
                if let Content::Loaded { scroll, .. } = &mut self.content {
                    *scroll += 1;
                }
            }
            KeyCode::Char('k') => {
                if let Content::Loaded { scroll, .. } = &mut self.content {
                    *scroll = scroll.saturating_sub(1);
                }
            }
            _ => {}
        }
        IssueEvent::Consumed
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let boxed = area.inner(INSET);
        Clear.render(boxed, buf);
        buf.set_style(boxed, theme::base());
        let block = Block::bordered()
            .style(theme::on_bg(COLOR_SCHEME.hi))
            .title(format!(" #{} {} ", self.number, self.title))
            .title_alignment(HorizontalAlignment::Center);
        let inner = block.inner(boxed).inner(Margin::new(1, 0));
        block.render(boxed, buf);
        match &mut self.content {
            Content::Loading => Paragraph::new("Loading issue..")
                .style(theme::base())
                .render(inner, buf),
            Content::Failed(message) => Paragraph::new(message.as_str())
                .style(theme::on_bg(COLOR_SCHEME.error))
                .render(inner, buf),
            Content::Loaded { issue, scroll } => render_issue(issue, scroll, inner, buf),
        }
    }
}

fn render_issue(issue: &Issue, scroll: &mut usize, area: Rect, buf: &mut Buffer) {
    let [header, badges, _gap, body] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);
    let state = match issue.state {
        IssueState::Open => "open",
        IssueState::Closed => "closed",
    };
    let author = issue.author.as_deref().unwrap_or("ghost");
    Paragraph::new(Line::from(vec![
        Span::styled(
            issue.repository.as_str(),
            theme::on_bg(COLOR_SCHEME.text_hi),
        ),
        Span::raw("  "),
        Span::styled(state, theme::on_bg(COLOR_SCHEME.hi)),
        Span::raw("  by "),
        Span::styled(format!("@{author}"), theme::on_bg(COLOR_SCHEME.text_hi)),
    ]))
    .style(theme::base())
    .render(header, buf);
    let mut spans: Vec<Span> = Vec::new();
    for label in &issue.labels {
        let bg = label_color(&label.color).unwrap_or(COLOR_SCHEME.hi_bg);
        spans.push(Span::styled(
            format!(" {} ", label.name),
            Style::default().fg(badge_text_color(bg)).bg(bg),
        ));
        spans.push(Span::raw(" "));
    }
    for login in &issue.assignees {
        spans.push(Span::styled(
            format!(" @{login} "),
            Style::default()
                .fg(COLOR_SCHEME.text_hi)
                .bg(COLOR_SCHEME.hi_bg),
        ));
        spans.push(Span::raw(" "));
    }
    Paragraph::new(Line::from(spans))
        .style(theme::base())
        .render(badges, buf);
    let lines: Vec<String> = issue
        .body
        .lines()
        .flat_map(|line| wrap_title(line, body.width as usize))
        .collect();
    *scroll = (*scroll).min(lines.len().saturating_sub(body.height as usize));
    let text: Vec<Line> = lines
        .into_iter()
        .skip(*scroll)
        .take(body.height as usize)
        .map(Line::from)
        .collect();
    Paragraph::new(text).style(theme::base()).render(body, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::board::Label;
    use crate::github::issue::IssueState;
    use crate::testkit::render_rows;

    fn issue(body: &str) -> Issue {
        Issue {
            number: 7,
            title: "Crash on start".into(),
            state: IssueState::Open,
            body: body.into(),
            url: "https://github.com/o/r/issues/7".into(),
            author: Some("octo".into()),
            repository: "o/r".into(),
            labels: vec![Label {
                name: "bug".into(),
                color: "d73a4a".into(),
            }],
            assignees: vec!["a".into()],
        }
    }

    #[test]
    /// TU-R-059 — the overlay is centered, covers most of the screen and reads `Loading issue..` until the details arrive; a failure shows its message.
    fn ut_loading_then_failure() {
        let mut d = IssueDialog::new(7, "Crash on start".into());
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows[1].contains("#7 Crash on start"), "{}", rows[1]);
        assert!(
            rows[1].starts_with("    ┌") || rows[1].starts_with("     ┌"),
            "centered: {}",
            rows[1]
        );
        assert!(
            rows.iter().any(|r| r.contains("Loading issue..")),
            "{rows:?}"
        );
        assert!(rows[22].contains('└'), "{}", rows[22]);
        d.set_result(Err::<Issue, _>("github: HTTP 401"));
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains("github: HTTP 401")),
            "{rows:?}"
        );
    }

    #[test]
    /// TU-R-060 — header line, badges and wrapped body; `j`/`k` scroll the body inside its bounds.
    fn ut_details_and_scrolling() {
        let mut d = IssueDialog::new(7, "Crash on start".into());
        let body = (1..=30)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        d.set_result(Ok::<_, String>(issue(&body)));
        let rows = render_rows(80, 14, |f| d.render(f.area(), f.buffer_mut()));
        let header = rows.iter().find(|r| r.contains("o/r")).expect("header");
        assert!(
            header.contains("open") && header.contains("@octo"),
            "{header}"
        );
        let badges = rows.iter().find(|r| r.contains("bug")).expect("badges");
        assert!(badges.contains("@a"), "{badges}");
        assert!(rows.iter().any(|r| r.contains("line 1")), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("line 30")), "{rows:?}");
        for _ in 0..100 {
            assert_eq!(d.handle_key(KeyCode::Char('j')), IssueEvent::Consumed);
        }
        let rows = render_rows(80, 14, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains("line 30")),
            "scrolled to the end: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("line 2")),
            "no scrolling past the end: {rows:?}"
        );
        for _ in 0..100 {
            d.handle_key(KeyCode::Char('k'));
        }
        let rows = render_rows(80, 14, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains("line 1")), "{rows:?}");
    }

    #[test]
    /// TU-R-060 — a body line wider than the overlay wraps instead of being cut.
    fn ut_body_wraps() {
        let mut d = IssueDialog::new(7, "T".into());
        d.set_result(Ok::<_, String>(issue(&"word ".repeat(40))));
        let rows = render_rows(60, 20, |f| d.render(f.area(), f.buffer_mut()));
        let word_rows = rows.iter().filter(|r| r.contains("word word")).count();
        assert!(word_rows >= 3, "{rows:?}");
    }

    #[test]
    /// TU-R-061 — Esc and `q` close; other keys are consumed.
    fn ut_close_keys() {
        let mut d = IssueDialog::new(7, "T".into());
        assert_eq!(d.handle_key(KeyCode::Esc), IssueEvent::Close);
        assert_eq!(d.handle_key(KeyCode::Char('q')), IssueEvent::Close);
        assert_eq!(d.handle_key(KeyCode::Char('x')), IssueEvent::Consumed);
        assert_eq!(d.handle_key(KeyCode::Enter), IssueEvent::Consumed);
    }
}
