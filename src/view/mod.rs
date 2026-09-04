//! Everything drawn on screen: the tab line and tab bodies, the command line, and dialogs.

pub mod board;
pub mod command_line;
pub mod dialog;
pub mod notice;
pub mod remote;
pub mod tabs;
pub mod theme;

use ratatui::layout::Rect;

/// Space kept free between a box's borders and its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Padding {
    pub vertical: u16,
    pub horizontal: u16,
}

impl Padding {
    /// `area` shrunk by the padding on every side; empty when the padding does not fit.
    pub fn inner(self, area: Rect) -> Rect {
        Rect {
            x: area.x + self.horizontal.min(area.width / 2),
            y: area.y + self.vertical.min(area.height / 2),
            width: area.width.saturating_sub(2 * self.horizontal),
            height: area.height.saturating_sub(2 * self.vertical),
        }
    }
}
