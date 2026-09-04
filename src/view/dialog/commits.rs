//! The `Commits` tab of the pull request overlay.

use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::github::pull::Commit;
use crate::view::theme;

/// One line per commit, the selected one on the highlight background and kept in view.
pub fn render(commits: &[Commit], selected: usize, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .style(theme::on_bg(theme::TEMPLATE.border))
        .title(" Commits ");
    let inner = block.inner(area).inner(Margin::new(1, 0));
    block.render(area, buf);
    if commits.is_empty() {
        Paragraph::new(Line::styled(
            "None",
            theme::on_bg(theme::TEMPLATE.placeholder),
        ))
        .style(theme::base())
        .render(inner, buf);
        return;
    }
    let height = inner.height as usize;
    let offset = selected.saturating_sub(height.saturating_sub(1));
    let width = inner.width as usize;
    let lines: Vec<Line<'static>> = commits
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(i, c)| {
            let right = format!(
                "@{}  {}",
                c.author,
                c.date.chars().take(10).collect::<String>()
            );
            let left = format!("{}  {}", c.sha, c.headline);
            let left_width = width.saturating_sub(right.chars().count() + 2);
            let left: String = left.chars().take(left_width).collect();
            let pad = left_width.saturating_sub(left.chars().count()) + 2;
            let line = Line::from(vec![
                Span::styled(left, theme::on_bg(theme::TEMPLATE.text)),
                Span::raw(" ".repeat(pad)),
                Span::styled(right, theme::on_bg(theme::TEMPLATE.placeholder)),
            ]);
            if i == selected {
                theme::highlighted(line)
            } else {
                line
            }
        })
        .collect();
    Paragraph::new(lines)
        .style(theme::base())
        .render(inner, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::render_buffer;
    use crate::view::theme;

    fn commit(i: u32) -> Commit {
        Commit {
            sha: format!("sha{i:04}"),
            headline: format!("Change {i}"),
            author: "octo".into(),
            date: format!("2026-09-{i:02}T10:00:00Z"),
        }
    }

    fn rows(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    /// TU-R-073 — a titled box lists id, headline, author and date per commit; the selected line carries the highlight background and the list scrolls to it.
    fn ut_render_lists_and_scrolls() {
        let commits: Vec<Commit> = (1..=6).map(commit).collect();
        let buf = render_buffer(60, 5, |f| render(&commits, 0, f.area(), f.buffer_mut()));
        let lines = rows(&buf);
        assert!(lines[0].contains(" Commits "), "{lines:?}");
        assert!(
            lines[1].contains("sha0001") && lines[1].contains("Change 1"),
            "{lines:?}"
        );
        assert!(
            lines[1].contains("@octo") && lines[1].ends_with("2026-09-01 │"),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("sha0004")),
            "cut at the box: {lines:?}"
        );
        let x = lines[1].find("sha0001").expect("id") as u16;
        assert_eq!(buf[(x, 1)].bg, theme::TEMPLATE.hi_bg);
        let buf = render_buffer(60, 5, |f| render(&commits, 5, f.area(), f.buffer_mut()));
        let lines = rows(&buf);
        assert!(lines[3].contains("sha0006"), "scrolled: {lines:?}");
        let x = lines[3].find("sha0006").expect("id") as u16;
        assert_eq!(buf[(x, 3)].bg, theme::TEMPLATE.hi_bg);
        assert_ne!(buf[(x, 2)].bg, theme::TEMPLATE.hi_bg);
        let buf = render_buffer(60, 5, |f| render(&[], 0, f.area(), f.buffer_mut()));
        assert!(rows(&buf)[1].contains("None"), "{:?}", rows(&buf));
    }
}
