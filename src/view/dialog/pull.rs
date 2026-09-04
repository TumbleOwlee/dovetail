//! Pull request details overlay: a description card and a comments card.

use crossterm::event::KeyCode;
use ferrowl_ui::COLOR_SCHEME;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, HorizontalAlignment, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::github::pull::{PullDetails, ReviewState};
use crate::view::board::wrap_title;
use crate::view::board::{badge_text_color, label_color};
use crate::view::theme;

/// Screen cells left free around the overlay on each side.
const INSET: Margin = Margin::new(4, 1);

/// Space between a card's border and its text.
const CARD_MARGIN: Margin = Margin::new(1, 0);

/// Columns of the bar at the overlay's right.
const BAR_WIDTH: u16 = 30;

/// What the caller does after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullEvent {
    Consumed,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    Loading,
    Failed(String),
    Loaded {
        details: Box<PullDetails>,
        scroll: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullDialog {
    number: u64,
    title: String,
    content: Content,
}

impl PullDialog {
    /// Opens in the loading state for the row's pull request.
    pub fn new(number: u64, title: String) -> PullDialog {
        PullDialog {
            number,
            title,
            content: Content::Loading,
        }
    }

    /// Replaces the loading state with the details or the failure.
    pub fn set_result(&mut self, result: Result<PullDetails, impl ToString>) {
        self.content = match result {
            Ok(details) => Content::Loaded {
                details: Box::new(details),
                scroll: 0,
            },
            Err(e) => Content::Failed(e.to_string()),
        };
    }

    pub fn handle_key(&mut self, code: KeyCode) -> PullEvent {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => return PullEvent::Close,
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
        PullEvent::Consumed
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
            Content::Loading => Paragraph::new("Loading pull request..")
                .style(theme::base())
                .render(inner, buf),
            Content::Failed(message) => Paragraph::new(message.as_str())
                .style(theme::on_bg(COLOR_SCHEME.error))
                .render(inner, buf),
            Content::Loaded { details, scroll } => {
                let [left, bar] =
                    Layout::horizontal([Constraint::Min(0), Constraint::Length(BAR_WIDTH)])
                        .areas(inner);
                render_cards(details, scroll, left, buf);
                render_bar(details, bar, buf);
            }
        }
    }
}

/// A bordered card's text: title line, then its lines.
struct CardText {
    title: String,
    lines: Vec<Line<'static>>,
}

impl CardText {
    /// Rows the card takes with its borders.
    fn height(&self) -> u16 {
        self.lines.len() as u16 + 2
    }

    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .style(theme::on_bg(COLOR_SCHEME.border))
            .title(self.title);
        let inner = block.inner(area).inner(CARD_MARGIN);
        block.render(area, buf);
        Paragraph::new(self.lines)
            .style(theme::base())
            .render(inner, buf);
    }
}

fn wrapped(text: &str, width: usize) -> impl Iterator<Item = Line<'static>> + '_ {
    text.lines()
        .flat_map(move |line| wrap_title(line, width))
        .map(Line::from)
}

fn review_state(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Pending => "pending",
        ReviewState::Approved => "approved",
        ReviewState::ChangesRequested => "changes requested",
        ReviewState::Commented => "commented",
        ReviewState::Dismissed => "dismissed",
    }
}

/// The seven boxes stacked top to bottom, each as tall as its entries, cut at the bottom.
fn render_bar(details: &PullDetails, area: Rect, buf: &mut Buffer) {
    let logins = |names: &[String]| -> Vec<Line<'static>> {
        names.iter().map(|n| Line::raw(format!("@{n}"))).collect()
    };
    let reviewers = details
        .reviewers
        .iter()
        .map(|r| {
            Line::from(vec![
                Span::raw(format!("@{} ", r.name)),
                Span::styled(
                    review_state(r.state),
                    theme::on_bg(COLOR_SCHEME.placeholder),
                ),
            ])
        })
        .collect();
    let labels = details
        .labels
        .iter()
        .map(|l| {
            let bg = label_color(&l.color).unwrap_or(COLOR_SCHEME.hi_bg);
            Line::from(Span::styled(
                format!(" {} ", l.name),
                Style::default().fg(badge_text_color(bg)).bg(bg),
            ))
        })
        .collect();
    let plain = |items: &[String]| -> Vec<Line<'static>> {
        items.iter().map(|i| Line::raw(i.clone())).collect()
    };
    let boxes: [(&str, Vec<Line<'static>>); 7] = [
        ("Reviewers", reviewers),
        ("Assignees", logins(&details.assignees)),
        ("Labels", labels),
        ("Projects", plain(&details.projects)),
        ("Milestone", plain(details.milestone.as_slice())),
        ("Development", plain(&details.development)),
        ("Participants", logins(&details.participants)),
    ];
    let mut y = area.y;
    for (title, mut lines) in boxes {
        if lines.is_empty() {
            lines.push(Line::styled("None", theme::on_bg(COLOR_SCHEME.placeholder)));
        }
        let card = CardText {
            title: format!(" {title} "),
            lines,
        };
        let height = card.height().min(area.bottom().saturating_sub(y));
        if height < 2 {
            break;
        }
        card.render(Rect::new(area.x, y, area.width, height), buf);
        y += height;
    }
}

/// Both cards stacked in a buffer as tall as they need, scrolled into `area`.
fn render_cards(details: &PullDetails, scroll: &mut usize, area: Rect, buf: &mut Buffer) {
    let text_width = area.width.saturating_sub(2 + 2 * CARD_MARGIN.horizontal) as usize;
    let mut description: Vec<Line<'static>> = vec![
        Line::styled(details.title.clone(), theme::on_bg(COLOR_SCHEME.text_hi)),
        Line::raw(""),
    ];
    description.extend(wrapped(&details.body, text_width));
    let mut comments: Vec<Line<'static>> = Vec::new();
    if details.comments.is_empty() {
        comments.push(Line::raw("No comments"));
    }
    for (i, comment) in details.comments.iter().enumerate() {
        if i > 0 {
            comments.push(Line::raw(""));
        }
        let author = comment.author.as_deref().unwrap_or("ghost");
        let date: String = comment.created_at.chars().take(10).collect();
        comments.push(Line::from(vec![
            Span::styled(format!("@{author}"), theme::on_bg(COLOR_SCHEME.hi)),
            Span::raw("  "),
            Span::styled(date, theme::on_bg(COLOR_SCHEME.placeholder)),
        ]));
        comments.extend(wrapped(&comment.body, text_width));
    }
    let cards = [
        CardText {
            title: format!(" #{} ", details.number),
            lines: description,
        },
        CardText {
            title: format!(" Comments ({}) ", details.comments.len()),
            lines: comments,
        },
    ];
    let total: u16 = cards.iter().map(CardText::height).sum();
    let mut canvas = Buffer::empty(Rect::new(0, 0, area.width, total));
    canvas.set_style(canvas.area, theme::base());
    let mut y = 0;
    for card in cards {
        let height = card.height();
        card.render(Rect::new(0, y, area.width, height), &mut canvas);
        y += height;
    }
    *scroll = (*scroll).min((total as usize).saturating_sub(area.height as usize));
    for row in 0..area.height.min(total) {
        for x in 0..area.width {
            buf[(area.x + x, area.y + row)] = canvas[(x, row + *scroll as u16)].clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::board::Label;
    use crate::github::pull::{Comment, ReviewState, Reviewer};
    use crate::github::pulls::PullState;
    use crate::testkit::render_rows;

    fn details(body: &str, comments: Vec<Comment>) -> PullDetails {
        PullDetails {
            number: 5,
            title: "Fix crash".into(),
            body: body.into(),
            state: PullState::Open,
            draft: false,
            url: "https://github.com/o/r/pull/5".into(),
            author: Some("octo".into()),
            comments,
            ..PullDetails::default()
        }
    }

    #[test]
    /// TU-R-068, TU-E-026 — a 30 column bar at the right stacks the seven boxes, `None` for empty ones, clipped at the bottom.
    fn ut_right_bar_boxes() {
        let mut d = PullDialog::new(5, "Fix crash".into());
        let mut full = details("body", vec![]);
        full.reviewers = vec![
            Reviewer {
                name: "rev".into(),
                state: ReviewState::Pending,
            },
            Reviewer {
                name: "a".into(),
                state: ReviewState::ChangesRequested,
            },
        ];
        full.assignees = vec!["b".into()];
        full.labels = vec![Label {
            name: "bug".into(),
            color: "d73a4a".into(),
        }];
        full.projects = vec!["Roadmap".into()];
        full.milestone = Some("v1".into());
        full.development = vec!["#7 Crash on start".into()];
        full.participants = vec!["octo".into(), "a".into()];
        d.set_result(Ok::<_, String>(full));
        let rows = render_rows(100, 40, |f| d.render(f.area(), f.buffer_mut()));
        let bar_left = 100 - 4 - 1 - 30;
        let bar: Vec<String> = rows
            .iter()
            .map(|r| r.chars().skip(bar_left).take(30).collect())
            .collect();
        let titles = [
            "Reviewers",
            "Assignees",
            "Labels",
            "Projects",
            "Milestone",
            "Development",
            "Participants",
        ];
        let mut last = 0;
        for title in titles {
            let at = bar
                .iter()
                .position(|r| r.contains(title))
                .unwrap_or_else(|| panic!("{title}: {bar:?}"));
            assert!(
                at > last || last == 0,
                "{title} below the previous box: {bar:?}"
            );
            last = at;
        }
        let at = |text: &str| {
            bar.iter()
                .position(|r| r.contains(text))
                .unwrap_or_else(|| panic!("{text}: {bar:?}"))
        };
        assert!(bar[at("@rev")].contains("pending"), "{}", bar[at("@rev")]);
        assert!(
            bar[at("@a ")].contains("changes requested"),
            "{}",
            bar[at("@a ")]
        );
        assert!(bar.iter().any(|r| r.contains("@b")), "{bar:?}");
        assert!(bar.iter().any(|r| r.contains("bug")), "{bar:?}");
        assert!(bar.iter().any(|r| r.contains("Roadmap")), "{bar:?}");
        assert!(bar.iter().any(|r| r.contains("v1")), "{bar:?}");
        assert!(
            bar.iter().any(|r| r.contains("#7 Crash on start")),
            "{bar:?}"
        );
        assert!(bar.iter().any(|r| r.contains("@octo")), "{bar:?}");
        assert!(
            rows.iter().any(|r| r.contains("Comments (0)")),
            "left content still there: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|r| r.contains("Fix crash") && r.find("Fix crash").expect("title") < bar_left),
            "{rows:?}"
        );

        let mut d = PullDialog::new(5, "T".into());
        d.set_result(Ok::<_, String>(details("body", vec![])));
        let rows = render_rows(100, 40, |f| d.render(f.area(), f.buffer_mut()));
        let bar: Vec<String> = rows
            .iter()
            .map(|r| r.chars().skip(bar_left).take(30).collect())
            .collect();
        assert_eq!(
            bar.iter().filter(|r| r.contains("None")).count(),
            7,
            "{bar:?}"
        );

        let rows = render_rows(100, 12, |f| d.render(f.area(), f.buffer_mut()));
        let bar: Vec<String> = rows
            .iter()
            .map(|r| r.chars().skip(bar_left).take(30).collect())
            .collect();
        assert!(bar.iter().any(|r| r.contains("Reviewers")), "{bar:?}");
        assert!(
            !bar.iter().any(|r| r.contains("Participants")),
            "clipped: {bar:?}"
        );
        assert!(rows.iter().all(|r| r.chars().count() <= 100), "{rows:?}");
    }

    fn comment(author: Option<&str>, body: &str) -> Comment {
        Comment {
            author: author.map(String::from),
            created_at: "2026-09-04T10:00:00Z".into(),
            body: body.into(),
        }
    }

    #[test]
    /// TU-R-065 — centered overlay reading `Loading pull request..` until the details arrive; a failure shows its message.
    fn ut_loading_then_failure() {
        let mut d = PullDialog::new(5, "Fix crash".into());
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows[1].contains("#5 Fix crash"), "{}", rows[1]);
        assert!(rows[1].starts_with("    ┌"), "centered: {}", rows[1]);
        assert!(
            rows.iter().any(|r| r.contains("Loading pull request..")),
            "{rows:?}"
        );
        assert!(rows[22].contains('└'), "{}", rows[22]);
        d.set_result(Err::<PullDetails, _>("github: HTTP 401"));
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains("github: HTTP 401")),
            "{rows:?}"
        );
    }

    #[test]
    /// TU-R-066 — description card with title and body, comments card below with author, date and body; `j`/`k` scroll within bounds.
    fn ut_cards_and_scrolling() {
        let mut d = PullDialog::new(5, "Fix crash".into());
        let comments = vec![comment(Some("a"), "LGTM"), comment(None, "ghost says hi")];
        d.set_result(Ok::<_, String>(details(
            "Fixes the crash on start.",
            comments,
        )));
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        let card = rows
            .iter()
            .position(|r| r.contains("┌") && r.contains("#5") && !r.contains("Fix crash"))
            .expect("description card title");
        assert!(rows[card + 1].contains("Fix crash"), "{}", rows[card + 1]);
        assert!(
            rows[card + 2]
                .chars()
                .take(44)
                .all(|c| c == '│' || c == ' '),
            "empty line: {}",
            rows[card + 2]
        );
        assert!(
            rows[card + 3].contains("Fixes the crash on start."),
            "{}",
            rows[card + 3]
        );
        let comments_card = rows
            .iter()
            .position(|r| r.contains("Comments (2)"))
            .expect("comments card");
        assert!(comments_card > card + 3, "{rows:?}");
        assert!(
            rows[comments_card + 1].contains("@a")
                && rows[comments_card + 1].contains("2026-09-04"),
            "{}",
            rows[comments_card + 1]
        );
        assert!(
            rows[comments_card + 2].contains("LGTM"),
            "{}",
            rows[comments_card + 2]
        );
        assert!(
            rows[comments_card + 3]
                .chars()
                .take(44)
                .all(|c| c == '│' || c == ' '),
            "{}",
            rows[comments_card + 3]
        );
        assert!(
            rows[comments_card + 4].contains("@ghost"),
            "{}",
            rows[comments_card + 4]
        );
        assert!(
            rows[comments_card + 5].contains("ghost says hi"),
            "{}",
            rows[comments_card + 5]
        );

        let mut d = PullDialog::new(5, "Fix crash".into());
        let many: Vec<Comment> = (1..=30)
            .map(|n| comment(Some("a"), &format!("comment {n}")))
            .collect();
        d.set_result(Ok::<_, String>(details("body", many)));
        let rows = render_rows(80, 16, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains("comment 1")), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("comment 30")), "{rows:?}");
        for _ in 0..500 {
            assert_eq!(d.handle_key(KeyCode::Char('j')), PullEvent::Consumed);
        }
        let rows = render_rows(80, 16, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains("comment 30")),
            "scrolled to the end: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("comment 2")),
            "no scrolling past the end: {rows:?}"
        );
        for _ in 0..500 {
            d.handle_key(KeyCode::Char('k'));
        }
        let rows = render_rows(80, 16, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains("Fix crash")), "{rows:?}");
    }

    #[test]
    /// TU-E-024 — without comments the card reads `No comments`.
    fn ut_no_comments() {
        let mut d = PullDialog::new(5, "T".into());
        d.set_result(Ok::<_, String>(details("b", vec![])));
        let rows = render_rows(80, 20, |f| d.render(f.area(), f.buffer_mut()));
        let card = rows
            .iter()
            .position(|r| r.contains("Comments (0)"))
            .expect("card");
        assert!(rows[card + 1].contains("No comments"), "{}", rows[card + 1]);
    }

    #[test]
    /// TU-R-067 — Esc and `q` close; other keys are consumed.
    fn ut_close_keys() {
        let mut d = PullDialog::new(5, "T".into());
        assert_eq!(d.handle_key(KeyCode::Esc), PullEvent::Close);
        assert_eq!(d.handle_key(KeyCode::Char('q')), PullEvent::Close);
        assert_eq!(d.handle_key(KeyCode::Char('x')), PullEvent::Consumed);
    }
}
