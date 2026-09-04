//! The Task Board tab body: columns of cards with a single selection.

use crossterm::event::KeyCode;
use ferrowl_ui::COLOR_SCHEME;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::github::{Board, Card};
use crate::view::theme;

pub struct BoardView {
    board: Board,
    /// Column and card index of the selection; `None` when the board has no cards.
    selected: Option<(usize, usize)>,
}

impl BoardView {
    /// Selects the first card of the first non-empty column.
    pub fn new(board: Board) -> BoardView {
        let selected = board
            .columns
            .iter()
            .position(|c| !c.cards.is_empty())
            .map(|column| (column, 0));
        BoardView { board, selected }
    }

    #[cfg(test)]
    pub fn selected(&self) -> Option<(usize, usize)> {
        self.selected
    }

    #[cfg(test)]
    pub fn selected_card(&self) -> Option<&Card> {
        let (column, card) = self.selected?;
        self.board.columns.get(column)?.cards.get(card)
    }

    /// `h`/`l` move across non-empty columns to the nearest card index, `j`/`k` within a column.
    pub fn handle_key(&mut self, code: KeyCode) {
        let Some((column, card)) = self.selected else {
            return;
        };
        let columns = &self.board.columns;
        let next_non_empty = (column + 1..columns.len()).find(|i| !columns[*i].cards.is_empty());
        let prev_non_empty = (0..column).rev().find(|i| !columns[*i].cards.is_empty());
        self.selected = Some(match code {
            KeyCode::Char('j') | KeyCode::Down => {
                (column, (card + 1).min(columns[column].cards.len() - 1))
            }
            KeyCode::Char('k') | KeyCode::Up => (column, card.saturating_sub(1)),
            KeyCode::Char('l') | KeyCode::Right => match next_non_empty {
                Some(next) => (next, card.min(columns[next].cards.len() - 1)),
                None => (column, card),
            },
            KeyCode::Char('h') | KeyCode::Left => match prev_non_empty {
                Some(prev) => (prev, card.min(columns[prev].cards.len() - 1)),
                None => (column, card),
            },
            _ => (column, card),
        });
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let base = Style::default().fg(COLOR_SCHEME.text).bg(theme::BG);
        buf.set_style(area, base);
        if self.board.columns.is_empty() || area.height == 0 {
            return;
        }
        let widths = vec![Constraint::Fill(1); self.board.columns.len()];
        let columns = Layout::horizontal(widths).split(area);
        for (i, (column, rect)) in self.board.columns.iter().zip(columns.iter()).enumerate() {
            let block = Block::bordered()
                .style(Style::default().fg(COLOR_SCHEME.border).bg(theme::BG))
                .title(Span::styled(
                    format!(" {} ({}) ", column.name, column.cards.len()),
                    Style::default().fg(COLOR_SCHEME.hi).bg(theme::BG),
                ));
            let body = block.inner(*rect);
            block.render(*rect, buf);
            let selected_here = self.selected.filter(|(c, _)| *c == i).map(|(_, card)| card);
            let title_width = body.width.saturating_sub(4) as usize;
            let heights: Vec<u16> = column
                .cards
                .iter()
                .map(|card| card_height(card, title_width))
                .collect();
            let offset = scroll_offset(&heights, selected_here, body.height);
            let mut y = body.y;
            for (n, card) in column.cards.iter().enumerate().skip(offset) {
                let height = heights[n];
                if y + height > body.bottom() {
                    break;
                }
                let rect = Rect {
                    x: body.x,
                    y,
                    width: body.width,
                    height,
                };
                render_card(card, rect, buf, selected_here == Some(n));
                y += height;
            }
        }
    }
}

/// Bordered box centered in `area` for the outstanding board request.
pub fn render_loading(area: Rect, buf: &mut Buffer) {
    const MESSAGE: &str = "Board is loading..";
    buf.set_style(area, Style::default().fg(COLOR_SCHEME.text).bg(theme::BG));
    let width = (MESSAGE.len() as u16 + 4).min(area.width);
    let height = 3.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    let block = Block::bordered().style(theme::on_bg(COLOR_SCHEME.hi));
    let inner = block.inner(rect).inner(Margin::new(1, 0));
    block.render(rect, buf);
    Paragraph::new(MESSAGE)
        .style(theme::on_bg(COLOR_SCHEME.hi))
        .render(inner, buf);
}

/// Rows the card takes: border, wrapped title lines, badge line, border.
fn card_height(card: &Card, title_width: usize) -> u16 {
    wrap_title(&card.title, title_width).len() as u16 + 3
}

/// First card index to draw so the selected card ends inside `height` rows.
fn scroll_offset(heights: &[u16], selected: Option<usize>, height: u16) -> usize {
    let Some(selected) = selected else {
        return 0;
    };
    let mut offset = 0;
    while offset < selected && heights[offset..=selected].iter().sum::<u16>() > height {
        offset += 1;
    }
    offset
}

/// Word-wrap `title` to `width` columns; a word wider than `width` is broken at the width.
fn wrap_title(title: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;
    for word in title.split_whitespace() {
        let word_len = word.chars().count();
        if current_len > 0 && current_len + 1 + word_len <= width {
            current.push(' ');
            current.push_str(word);
            current_len += 1 + word_len;
            continue;
        }
        if current_len > 0 {
            lines.push(std::mem::take(&mut current));
            current_len = 0;
        }
        let chars: Vec<char> = word.chars().collect();
        for chunk in chars.chunks(width) {
            if current_len > 0 {
                lines.push(std::mem::take(&mut current));
            }
            current = chunk.iter().collect();
            current_len = chunk.len();
        }
    }
    lines.push(current);
    lines
}

fn render_card(card: &Card, area: Rect, buf: &mut Buffer, highlighted: bool) {
    let border = if highlighted {
        Style::default().fg(COLOR_SCHEME.hi).bg(theme::BG)
    } else {
        Style::default().fg(COLOR_SCHEME.border).bg(theme::BG)
    };
    let mut assignees: Vec<Span> = Vec::new();
    for (i, login) in card.assignees.iter().enumerate() {
        if i > 0 {
            assignees.push(Span::styled(" ", border));
        }
        assignees.push(Span::styled(
            format!(" @{login} "),
            Style::default()
                .fg(COLOR_SCHEME.text_hi)
                .bg(COLOR_SCHEME.hi_bg),
        ));
    }
    let block = Block::bordered()
        .style(border)
        .title_top(Line::from(format!("#{}", card.number)).left_aligned())
        .title_top(Line::from(assignees).right_aligned());
    let inner = block.inner(area).inner(Margin::new(1, 0));
    block.render(area, buf);
    let lines = wrap_title(&card.title, inner.width as usize);
    let [title, badges] = Layout::vertical([
        Constraint::Length(lines.len() as u16),
        Constraint::Length(1),
    ])
    .areas(inner);
    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();
    Paragraph::new(text)
        .style(Style::default().fg(COLOR_SCHEME.text).bg(theme::BG))
        .render(title, buf);
    let mut spans: Vec<Span> = Vec::new();
    for label in &card.labels {
        let bg = label_color(&label.color).unwrap_or(COLOR_SCHEME.hi_bg);
        spans.push(Span::styled(
            format!(" {} ", label.name),
            Style::default().fg(badge_text_color(bg)).bg(bg),
        ));
        spans.push(Span::raw(" "));
    }
    Paragraph::new(Line::from(spans)).render(badges, buf);
}

/// `rrggbb` to a terminal color; `None` for anything else.
pub fn label_color(hex: &str) -> Option<Color> {
    if hex.len() != 6 {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?))
}

/// Black on light backgrounds, white on dark ones.
pub fn badge_text_color(background: Color) -> Color {
    match background {
        Color::Rgb(r, g, b) => {
            let luminance = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
            if luminance > 150.0 {
                Color::Black
            } else {
                Color::White
            }
        }
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::board::{Column, Label};
    use crate::testkit::render_rows;

    fn card(number: u64, title: &str, labels: &[(&str, &str)], assignees: &[&str]) -> Card {
        Card {
            number,
            title: title.into(),
            labels: labels
                .iter()
                .map(|(n, c)| Label {
                    name: n.to_string(),
                    color: c.to_string(),
                })
                .collect(),
            assignees: assignees.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn column(name: &str, cards: Vec<Card>) -> Column {
        Column {
            name: name.into(),
            cards,
        }
    }

    fn board() -> Board {
        Board {
            title: "Roadmap".into(),
            columns: vec![
                column(
                    "Todo",
                    vec![
                        card(1, "First", &[("bug", "d73a4a")], &["octo"]),
                        card(2, "Second", &[], &[]),
                    ],
                ),
                column("Doing", vec![]),
                column(
                    "Done",
                    vec![card(
                        3,
                        "Third",
                        &[("docs", "0075ca"), ("good first issue", "7057ff")],
                        &["a", "b"],
                    )],
                ),
                column("No status", vec![]),
            ],
        }
    }

    #[test]
    /// TU-R-054 — the first card of the first non-empty column starts selected.
    fn ut_initial_selection_is_first_card() {
        let v = BoardView::new(board());
        assert_eq!(v.selected(), Some((0, 0)));
        assert_eq!(v.selected_card().map(|c| c.number), Some(1));
    }

    #[test]
    /// TU-R-054 — j/k move within a column, h/l across non-empty columns, none of them wrap.
    fn ut_navigation_moves_without_wrapping() {
        let mut v = BoardView::new(board());
        v.handle_key(KeyCode::Char('j'));
        assert_eq!(v.selected(), Some((0, 1)));
        v.handle_key(KeyCode::Char('j'));
        assert_eq!(v.selected(), Some((0, 1)));
        v.handle_key(KeyCode::Char('l')); // skips the empty Doing column, clamps the card index
        assert_eq!(v.selected(), Some((2, 0)));
        v.handle_key(KeyCode::Char('l')); // No status is empty: stay
        assert_eq!(v.selected(), Some((2, 0)));
        v.handle_key(KeyCode::Char('h'));
        assert_eq!(v.selected(), Some((0, 0)));
        v.handle_key(KeyCode::Char('k'));
        assert_eq!(v.selected(), Some((0, 0)));
        v.handle_key(KeyCode::Char('h'));
        assert_eq!(v.selected(), Some((0, 0)));
    }

    #[test]
    /// TU-E-016 — an empty board has no selection and ignores navigation.
    fn ut_empty_board_has_no_selection() {
        let mut v = BoardView::new(Board {
            title: "T".into(),
            columns: vec![column("Todo", vec![])],
        });
        assert_eq!(v.selected(), None);
        v.handle_key(KeyCode::Char('j'));
        v.handle_key(KeyCode::Char('l'));
        assert_eq!(v.selected(), None);
        let rows = render_rows(40, 6, |f| v.render(f.area(), f.buffer_mut()));
        assert!(rows[0].contains("Todo (0)"), "{}", rows[0]);
    }

    #[test]
    /// TU-R-053 — a label badge sits on its label color with readable text; bad hex has no color.
    fn ut_label_colors() {
        assert_eq!(label_color("d73a4a"), Some(Color::Rgb(0xd7, 0x3a, 0x4a)));
        assert_eq!(label_color("zz0000"), None);
        assert_eq!(label_color("fff"), None);
        assert_eq!(badge_text_color(Color::Rgb(0xff, 0xff, 0xff)), Color::Black);
        assert_eq!(badge_text_color(Color::Rgb(0x10, 0x10, 0x10)), Color::White);
        let v = BoardView::new(board());
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 12)).expect("backend");
        terminal
            .draw(|f| v.render(f.area(), f.buffer_mut()))
            .expect("draw");
        let buf = terminal.backend().buffer();
        let x = (0..200u16)
            .find(|x| buf[(*x, 3)].symbol() == "b" && buf[(*x + 1, 3)].symbol() == "u")
            .expect("bug badge");
        assert_eq!(buf[(x, 3)].bg, Color::Rgb(0xd7, 0x3a, 0x4a));
    }

    #[test]
    /// TU-R-055 — a column scrolls so the selected card stays visible.
    fn ut_column_scrolls_to_selection() {
        let cards = (1..=6)
            .map(|n| card(n, &format!("Card{n}"), &[], &[]))
            .collect();
        let mut v = BoardView::new(Board {
            title: "T".into(),
            columns: vec![column("Todo", cards)],
        });
        for _ in 0..5 {
            v.handle_key(KeyCode::Char('j'));
        }
        assert_eq!(v.selected(), Some((0, 5)));
        let rows = render_rows(30, 10, |f| v.render(f.area(), f.buffer_mut()));
        let joined = rows.join("\n");
        assert!(joined.contains("Card6"), "{joined}");
        assert!(!joined.contains("Card1"), "{joined}");
    }

    #[test]
    /// TU-R-051, TU-R-052, TU-R-053, TU-R-057 — bordered full-height columns titled with name and count; cards with number and assignees in the top border, title and label badges inside.
    fn ut_render_columns_and_cards() {
        let v = BoardView::new(board());
        let rows = render_rows(200, 12, |f| v.render(f.area(), f.buffer_mut()));
        let header = &rows[0];
        for h in ["Todo (2)", "Doing (0)", "Done (1)", "No status (0)"] {
            assert!(header.contains(h), "{header}");
        }
        assert!(header.find("Todo").expect("todo") < header.find("Done").expect("done"));
        assert!(header.starts_with('┌'), "{header}");
        assert!(
            rows[11].starts_with('└'),
            "column border spans the full height: {}",
            rows[11]
        );
        assert!(rows[5].contains('│'), "{}", rows[5]);
        assert!(
            rows[2].contains("First") && rows[2].contains("Third"),
            "{}",
            rows[2]
        );
        let todo = rows[1]
            .find("#1")
            .expect("issue number in the top-left corner");
        let done = rows[1]
            .find("#3")
            .expect("issue number in the top-left corner");
        assert!(
            todo < rows[1].find("@octo").expect("assignee top-right"),
            "{}",
            rows[1]
        );
        assert!(rows[1].find("@octo").expect("octo") < done, "{}", rows[1]);
        assert!(
            rows[1].find("@a").expect("a") < rows[1].find("@b").expect("b"),
            "{}",
            rows[1]
        );
        assert!(rows[1].contains("┌#1"), "number at the corner: {}", rows[1]);
        assert!(
            rows[1].contains("@b ┐"),
            "assignee at the corner: {}",
            rows[1]
        );
        assert!(!rows[3].contains("@octo"), "{}", rows[3]);
        assert!(rows[3].contains("bug"), "{}", rows[3]);
        assert!(
            rows[3].contains("docs") && rows[3].contains("good first issue"),
            "{}",
            rows[3]
        );
        assert!(rows[6].contains("Second"), "{}", rows[6]);
    }

    #[test]
    /// TU-R-052, TU-E-017, TU-E-018 — titles wrap to the card content width inside a one-column margin, a long word breaks, badges truncate.
    fn ut_titles_wrap_and_badges_truncate() {
        let labels: Vec<(&str, &str)> = (0..10).map(|_| ("verylonglabelname", "000000")).collect();
        let v = BoardView::new(Board {
            title: "T".into(),
            columns: vec![column(
                "Todo",
                vec![
                    card(1, "a rather long title that needs wrapping", &labels, &[]),
                    card(2, &"x".repeat(60), &[], &[]),
                ],
            )],
        });
        let rows = render_rows(30, 16, |f| v.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().all(|r| r.chars().count() <= 30), "{rows:?}");
        assert!(
            rows[2].starts_with("││ a rather long title"),
            "margin inside the card: {}",
            rows[2]
        );
        assert!(rows[4].starts_with("││  verylonglabelname"), "{}", rows[4]);
        assert!(
            rows[3].ends_with(" ││"),
            "margin on the right: {:?}",
            rows[3]
        );
        assert!(rows[3].contains("wrapping"), "{}", rows[3]);
        assert!(rows[4].contains("verylonglabelname"), "{}", rows[4]);
        assert!(
            rows[5].contains('└'),
            "card closes after the badge line: {}",
            rows[5]
        );
        let xs: Vec<&String> = rows.iter().filter(|r| r.contains("xxxxxxxx")).collect();
        assert!(xs.len() >= 2, "long word broken over lines: {rows:?}");
        assert_eq!(wrap_title("ab cd", 10), vec!["ab cd"]);
        assert_eq!(wrap_title("ab cd", 3), vec!["ab", "cd"]);
        assert_eq!(wrap_title("abcdef", 4), vec!["abcd", "ef"]);
        assert_eq!(wrap_title("", 4), vec![""]);
    }

    #[test]
    /// TU-R-050, TU-R-058 — the loading box is centered and drawn in the highlight color on the background.
    fn ut_loading_box_is_highlighted() {
        let area = Rect::new(0, 0, 40, 7);
        let mut buf = Buffer::empty(area);
        render_loading(area, &mut buf);
        let row =
            |y: u16| -> String { (0..40).map(|x| buf[(x, y)].symbol().to_string()).collect() };
        assert!(row(3).contains("Board is loading.."), "{}", row(3));
        assert!(row(2).contains('┌') && row(4).contains('└'), "{}", row(2));
        let corner = row(2).find('┌').expect("corner") as u16;
        assert_eq!(buf[(corner, 2)].fg, COLOR_SCHEME.hi);
        let text = row(3).find('B').expect("text") as u16;
        assert_eq!(buf[(text, 3)].fg, COLOR_SCHEME.hi);
        assert_eq!(buf[(text, 3)].bg, theme::BG);
        assert_eq!(buf[(0, 0)].bg, theme::BG, "the whole body is painted");
    }
}
