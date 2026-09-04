//! Centered box for an outstanding request.

use ferrowl_ui::COLOR_SCHEME;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::view::theme;

/// Paints `area` and draws `message` in a bordered box centered in it, both in the highlight color.
pub fn render(area: Rect, buf: &mut Buffer, message: &str) {
    buf.set_style(area, theme::base());
    let width = (message.len() as u16 + 4).min(area.width);
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
    Paragraph::new(message)
        .style(theme::on_bg(COLOR_SCHEME.hi))
        .render(inner, buf);
}
