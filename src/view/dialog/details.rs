//! Details overlay shared by issues and pull requests: a description card and a comments card at
//! the left, scrolled together, and a bar of focusable boxes at the right.

use crossterm::event::KeyCode;
use ferrowl_ui::COLOR_SCHEME;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, HorizontalAlignment, Layout, Margin, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::github::pull::Comment;
use crate::view::board::wrap_title;
use crate::view::{notice, theme};

/// Screen cells left free around the overlay on each side.
const INSET: Margin = Margin::new(4, 3);

/// Space between a card's borders and its text: two columns, one row.
const CARD_MARGIN: Margin = Margin::new(2, 1);

/// Space between a bar box's borders and its entries: two columns, no rows.
const BOX_MARGIN: Margin = Margin::new(2, 0);

/// Columns of the bar at the overlay's right.
const BAR_WIDTH: u16 = 30;

/// One box of the right bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarBox {
    pub title: &'static str,
    /// One entry per line; empty renders as `None`.
    pub lines: Vec<Line<'static>>,
}

/// What the overlay shows once loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsContent {
    pub title: String,
    /// `open`, `closed`, `merged` or `draft`.
    pub state: &'static str,
    /// `None` when the author account was deleted.
    pub author: Option<String>,
    pub body: String,
    pub comments: Vec<Comment>,
    pub boxes: Vec<SidebarBox>,
}

/// What the caller does after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailsEvent {
    Consumed,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    Loading,
    Failed(String),
    Loaded {
        content: Box<DetailsContent>,
        scroll: usize,
        /// Index of the focused box.
        focus: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsDialog {
    number: u64,
    title: String,
    loading: &'static str,
    content: Content,
}

impl DetailsDialog {
    /// Opens in the loading state, showing `loading` until a result arrives.
    pub fn new(number: u64, title: String, loading: &'static str) -> DetailsDialog {
        DetailsDialog {
            number,
            title,
            loading,
            content: Content::Loading,
        }
    }

    /// Replaces the loading state with the content or the failure.
    pub fn set_result(&mut self, result: Result<DetailsContent, impl ToString>) {
        self.content = match result {
            Ok(content) => Content::Loaded {
                content: Box::new(content),
                scroll: 0,
                focus: 0,
            },
            Err(e) => Content::Failed(e.to_string()),
        };
    }

    pub fn handle_key(&mut self, code: KeyCode) -> DetailsEvent {
        if matches!(code, KeyCode::Esc | KeyCode::Char('q')) {
            return DetailsEvent::Close;
        }
        let Content::Loaded {
            content,
            scroll,
            focus,
        } = &mut self.content
        else {
            return DetailsEvent::Consumed;
        };
        let boxes = content.boxes.len().max(1);
        match code {
            KeyCode::Char('j') => *scroll += 1,
            KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::Tab => *focus = (*focus + 1) % boxes,
            KeyCode::BackTab => *focus = (*focus + boxes - 1) % boxes,
            _ => {}
        }
        DetailsEvent::Consumed
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
        let number = self.number;
        match &mut self.content {
            Content::Loading => notice::render_loading(inner, buf, self.loading),
            Content::Failed(message) => notice::render_error(inner, buf, message),
            Content::Loaded {
                content,
                scroll,
                focus,
            } => {
                let [left, bar] =
                    Layout::horizontal([Constraint::Min(0), Constraint::Length(BAR_WIDTH)])
                        .areas(inner);
                render_cards(content, number, scroll, left, buf);
                render_bar(&content.boxes, *focus, bar, buf);
            }
        }
    }
}

/// A bordered card's text: title line, then its lines.
struct CardText {
    title: String,
    margin: Margin,
    lines: Vec<Line<'static>>,
    focused: bool,
}

impl CardText {
    /// Rows the card takes with its borders and padding.
    fn height(&self) -> u16 {
        self.lines.len() as u16 + 2 + 2 * self.margin.vertical
    }

    fn render(self, area: Rect, buf: &mut Buffer) {
        let color = if self.focused {
            COLOR_SCHEME.hi
        } else {
            COLOR_SCHEME.border
        };
        let block = Block::bordered()
            .style(theme::on_bg(color))
            .title(self.title);
        let inner = block.inner(area).inner(self.margin);
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

/// The boxes stacked top to bottom, each as tall as its entries, cut at the bottom.
fn render_bar(boxes: &[SidebarBox], focus: usize, area: Rect, buf: &mut Buffer) {
    let mut y = area.y;
    for (i, item) in boxes.iter().enumerate() {
        let mut lines = item.lines.clone();
        if lines.is_empty() {
            lines.push(Line::styled("None", theme::on_bg(COLOR_SCHEME.placeholder)));
        }
        let card = CardText {
            title: format!(" {} ", item.title),
            margin: BOX_MARGIN,
            lines,
            focused: i == focus,
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
fn render_cards(
    content: &DetailsContent,
    number: u64,
    scroll: &mut usize,
    area: Rect,
    buf: &mut Buffer,
) {
    let text_width = area.width.saturating_sub(2 + 2 * CARD_MARGIN.horizontal) as usize;
    let author = content.author.as_deref().unwrap_or("ghost");
    let mut description: Vec<Line<'static>> = vec![
        Line::styled(content.title.clone(), theme::on_bg(COLOR_SCHEME.text_hi)),
        Line::from(vec![
            Span::styled(content.state, theme::on_bg(COLOR_SCHEME.hi)),
            Span::raw("  by "),
            Span::styled(format!("@{author}"), theme::on_bg(COLOR_SCHEME.text_hi)),
        ]),
        Line::raw(""),
    ];
    description.extend(wrapped(&content.body, text_width));
    let mut comments: Vec<Line<'static>> = Vec::new();
    if content.comments.is_empty() {
        comments.push(Line::raw("No comments"));
    }
    for (i, comment) in content.comments.iter().enumerate() {
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
            title: format!(" #{number} "),
            margin: CARD_MARGIN,
            lines: description,
            focused: false,
        },
        CardText {
            title: format!(" Comments ({}) ", content.comments.len()),
            margin: CARD_MARGIN,
            lines: comments,
            focused: false,
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
    use crate::testkit::render_rows;

    fn content(body: &str, comments: Vec<Comment>) -> DetailsContent {
        DetailsContent {
            title: "Fix crash".into(),
            state: "open",
            author: Some("octo".into()),
            body: body.into(),
            comments,
            boxes: vec![
                SidebarBox {
                    title: "Reviewers",
                    lines: vec![Line::raw("@rev pending")],
                },
                SidebarBox {
                    title: "Assignees",
                    lines: vec![],
                },
                SidebarBox {
                    title: "Labels",
                    lines: vec![Line::raw(" bug ")],
                },
            ],
        }
    }

    fn comment(author: Option<&str>, body: &str) -> Comment {
        Comment {
            author: author.map(String::from),
            created_at: "2026-09-04T10:00:00Z".into(),
            body: body.into(),
        }
    }

    /// Columns of the bar for an 80 column screen: inset 4, border 1, margin 1 on each side.
    const BAR_LEFT_80: usize = 80 - 4 - 1 - 1 - 30;

    fn bar_of(rows: &[String], left: usize) -> Vec<String> {
        rows.iter()
            .map(|r| r.chars().skip(left).take(30).collect())
            .collect()
    }

    #[test]
    /// TU-R-059, TU-R-065 — centered overlay reading the loading message until a result arrives; a failure shows its message.
    fn ut_loading_then_failure() {
        let mut d = DetailsDialog::new(5, "Fix crash".into(), "Loading thing..");
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows[3].contains("#5 Fix crash"), "{}", rows[3]);
        assert!(rows[3].starts_with("    ┌"), "centered: {}", rows[3]);
        assert!(
            rows[2].trim().is_empty() && rows[21].trim().is_empty(),
            "{rows:?}"
        );
        let at = rows
            .iter()
            .position(|r| r.contains("Loading thing.."))
            .expect("loading box");
        assert!(
            (10..=13).contains(&at),
            "box centered in the overlay: {rows:?}"
        );
        let chars: Vec<char> = rows[at].chars().collect();
        let text_at = chars
            .windows(15)
            .position(|w| w.iter().collect::<String>() == "Loading thing..")
            .expect("text");
        assert_eq!(
            chars[text_at - 2..text_at],
            ['│', ' '],
            "boxed: {}",
            rows[at]
        );
        assert!(
            rows[at - 1].contains('┌') && rows[at + 1].contains('└'),
            "{rows:?}"
        );
        assert!(
            rows[at - 1].find('┌').expect("corner") > 10,
            "small box, not the overlay border: {}",
            rows[at - 1]
        );
        assert!(rows[20].contains('└'), "{}", rows[20]);
        d.set_result(Err::<DetailsContent, _>("github: HTTP 401"));
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        let at = rows
            .iter()
            .position(|r| r.contains("github: HTTP 401"))
            .expect("error box");
        assert!(
            rows[at - 1].contains('┌') && rows[at + 1].contains('└'),
            "{rows:?}"
        );
    }

    #[test]
    /// TU-R-066 — description card with title, state and author line and body inside a 2 by 1 padding; comments card below; `j`/`k` scroll within bounds.
    fn ut_cards_and_scrolling() {
        let mut d = DetailsDialog::new(5, "Fix crash".into(), "L");
        let comments = vec![comment(Some("a"), "LGTM"), comment(None, "ghost says hi")];
        d.set_result(Ok::<_, String>(content(
            "Fixes the crash on start.",
            comments,
        )));
        let rows = render_rows(80, 30, |f| d.render(f.area(), f.buffer_mut()));
        let left: Vec<String> = rows
            .iter()
            .map(|r| r.chars().take(BAR_LEFT_80).collect())
            .collect();
        let card = left
            .iter()
            .position(|r| r.contains("┌ #5 "))
            .expect("description card title");
        let blank = |r: &String| r.chars().all(|c| c == '│' || c == ' ');
        assert!(
            blank(&left[card + 1]),
            "vertical padding: {}",
            left[card + 1]
        );
        assert!(
            left[card + 2].starts_with("    │ │  Fix crash"),
            "horizontal padding: {}",
            left[card + 2]
        );
        assert!(
            left[card + 3].contains("open") && left[card + 3].contains("by @octo"),
            "{}",
            left[card + 3]
        );
        assert!(blank(&left[card + 4]), "empty line: {}", left[card + 4]);
        assert!(
            left[card + 5].contains("Fixes the crash on start."),
            "{}",
            left[card + 5]
        );
        assert!(
            blank(&left[card + 6]),
            "vertical padding: {}",
            left[card + 6]
        );
        assert!(left[card + 7].contains('└'), "{}", left[card + 7]);
        let comments_card = left
            .iter()
            .position(|r| r.contains("Comments (2)"))
            .expect("comments card");
        assert_eq!(comments_card, card + 8, "{left:?}");
        assert!(
            left[comments_card + 2].contains("@a")
                && left[comments_card + 2].contains("2026-09-04"),
            "{}",
            left[comments_card + 2]
        );
        assert!(
            left[comments_card + 3].contains("LGTM"),
            "{}",
            left[comments_card + 3]
        );
        assert!(
            blank(&left[comments_card + 4]),
            "{}",
            left[comments_card + 4]
        );
        assert!(
            left[comments_card + 5].contains("@ghost"),
            "{}",
            left[comments_card + 5]
        );
        assert!(
            left[comments_card + 6].contains("ghost says hi"),
            "{}",
            left[comments_card + 6]
        );

        let mut d = DetailsDialog::new(5, "Fix crash".into(), "L");
        let many: Vec<Comment> = (1..=30)
            .map(|n| comment(Some("a"), &format!("comment {n}")))
            .collect();
        d.set_result(Ok::<_, String>(content("body", many)));
        let rows = render_rows(80, 20, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains("comment 1")), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("comment 30")), "{rows:?}");
        for _ in 0..500 {
            assert_eq!(d.handle_key(KeyCode::Char('j')), DetailsEvent::Consumed);
        }
        let rows = render_rows(80, 20, |f| d.render(f.area(), f.buffer_mut()));
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
        let rows = render_rows(80, 20, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains("Fix crash")), "{rows:?}");
    }

    #[test]
    /// TU-R-066 — a body line wider than the card wraps; without comments the card reads `No comments`; a deleted author reads `ghost`.
    fn ut_wrapping_and_empty_comments() {
        let mut d = DetailsDialog::new(5, "T".into(), "L");
        let mut c = content(&"word ".repeat(40), vec![]);
        c.author = None;
        d.set_result(Ok::<_, String>(c));
        let rows = render_rows(90, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().filter(|r| r.contains("word word")).count() >= 3,
            "{rows:?}"
        );
        assert!(rows.iter().all(|r| r.chars().count() <= 90), "{rows:?}");
        assert!(rows.iter().any(|r| r.contains("by @ghost")), "{rows:?}");
        let card = rows
            .iter()
            .position(|r| r.contains("Comments (0)"))
            .expect("card");
        assert!(rows[card + 2].contains("No comments"), "{}", rows[card + 2]);
    }

    #[test]
    /// TU-R-068, TU-R-069, TU-E-026 — the bar stacks the boxes with `None` for empty ones, clipped at the bottom; the first box is focused and Tab cycles the focus, the left content still scrolls.
    fn ut_bar_boxes_and_focus() {
        let mut d = DetailsDialog::new(5, "T".into(), "L");
        d.set_result(Ok::<_, String>(content("body", vec![])));
        let rows = render_rows(80, 30, |f| d.render(f.area(), f.buffer_mut()));
        let bar = bar_of(&rows, BAR_LEFT_80);
        let at = |title: &str| {
            bar.iter()
                .position(|r| r.contains(title))
                .unwrap_or_else(|| panic!("{title}: {bar:?}"))
        };
        assert!(
            at("Reviewers") < at("Assignees") && at("Assignees") < at("Labels"),
            "{bar:?}"
        );
        assert!(
            bar[at("Reviewers") + 1].contains("@rev pending"),
            "{}",
            bar[at("Reviewers") + 1]
        );
        assert!(
            bar[at("Assignees") + 1].contains("None"),
            "{}",
            bar[at("Assignees") + 1]
        );
        assert!(
            bar[at("Labels") + 1].contains("bug"),
            "{}",
            bar[at("Labels") + 1]
        );
        assert!(
            bar[at("Reviewers")].starts_with('┌'),
            "bar starts at its own border: {}",
            bar[at("Reviewers")]
        );

        let focused = |d: &mut DetailsDialog| -> Vec<&'static str> {
            let area = Rect::new(0, 0, 80, 30);
            let mut buf = Buffer::empty(area);
            d.render(area, &mut buf);
            let mut out = Vec::new();
            for (title, y) in [("Reviewers", 0), ("Assignees", 0), ("Labels", 0)]
                .iter()
                .map(|(t, _)| (*t, 0))
            {
                let _ = y;
                let row = (0..30u16)
                    .find(|y| {
                        (0..80u16).any(|x| {
                            let s: String = (x..(x + title.len() as u16).min(80))
                                .map(|xx| buf[(xx, *y)].symbol().to_string())
                                .collect();
                            s == title
                        })
                    })
                    .expect("box title row");
                let corner = (BAR_LEFT_80 as u16..80)
                    .find(|x| buf[(*x, row)].symbol() == "┌")
                    .expect("corner");
                if buf[(corner, row)].fg == COLOR_SCHEME.hi {
                    out.push(title);
                }
            }
            out
        };
        assert_eq!(focused(&mut d), vec!["Reviewers"]);
        assert_eq!(d.handle_key(KeyCode::Tab), DetailsEvent::Consumed);
        assert_eq!(focused(&mut d), vec!["Assignees"]);
        d.handle_key(KeyCode::Tab);
        assert_eq!(focused(&mut d), vec!["Labels"]);
        d.handle_key(KeyCode::Tab);
        assert_eq!(focused(&mut d), vec!["Reviewers"], "wraps to the first");
        d.handle_key(KeyCode::BackTab);
        assert_eq!(focused(&mut d), vec!["Labels"], "reverse wraps to the last");
        d.handle_key(KeyCode::Char('j'));
        assert_eq!(focused(&mut d), vec!["Labels"], "scrolling keeps the focus");

        let rows = render_rows(80, 13, |f| d.render(f.area(), f.buffer_mut()));
        let bar = bar_of(&rows, BAR_LEFT_80);
        assert!(bar.iter().any(|r| r.contains("Reviewers")), "{bar:?}");
        assert!(
            !bar.iter().any(|r| r.contains("Labels")),
            "clipped: {bar:?}"
        );
    }

    #[test]
    /// TU-R-061, TU-R-067 — Esc and `q` close; other keys are consumed.
    fn ut_close_keys() {
        let mut d = DetailsDialog::new(5, "T".into(), "L");
        assert_eq!(d.handle_key(KeyCode::Esc), DetailsEvent::Close);
        assert_eq!(d.handle_key(KeyCode::Char('q')), DetailsEvent::Close);
        assert_eq!(d.handle_key(KeyCode::Char('x')), DetailsEvent::Consumed);
        assert_eq!(d.handle_key(KeyCode::Tab), DetailsEvent::Consumed);
    }
}
