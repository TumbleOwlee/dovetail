//! Centered box for an outstanding request or its failure.

use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Color;
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::view::theme;

/// Paints `area` and draws `message` in a bordered box centered in it, both in the highlight color.
pub fn render_loading(area: Rect, buf: &mut Buffer, message: &str) {
    render(area, buf, message, theme::TEMPLATE.hi);
}

/// Paints `area` and draws `message` in a bordered box centered in it, both in the error color.
pub fn render_error(area: Rect, buf: &mut Buffer, message: &str) {
    render(area, buf, message, theme::TEMPLATE.error);
}

fn render(area: Rect, buf: &mut Buffer, message: &str, color: Color) {
    buf.set_style(area, theme::base());
    let width = (message.len() as u16 + 4).min(area.width);
    let height = 3.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    let block = Block::bordered().style(theme::on_bg(color));
    let inner = block.inner(rect).inner(Margin::new(1, 0));
    block.render(rect, buf);
    Paragraph::new(message)
        .style(theme::on_bg(color))
        .render(inner, buf);
}
