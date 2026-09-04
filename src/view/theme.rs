//! Palette overrides on top of ferrowl-ui's fixed color scheme.

use ferrowl_ui::COLOR_SCHEME;
use ferrowl_ui::style::{InputFieldStyle, ScrollingTabsStyle, SelectionStyle};
use ratatui::style::{Color, Style};

/// Background of every view, darker than the scheme's own.
pub const BG: Color = Color::Rgb(18, 18, 18);

/// Plain text on the background.
pub fn base() -> Style {
    Style::default().fg(COLOR_SCHEME.text).bg(BG)
}

/// `color` on the background.
pub fn on_bg(color: Color) -> Style {
    Style::default().fg(color).bg(BG)
}

/// The scheme's input field style on [`BG`].
pub fn input_field_style() -> InputFieldStyle {
    InputFieldStyle {
        general: base(),
        focused: on_bg(COLOR_SCHEME.hi),
        border: on_bg(COLOR_SCHEME.border),
        placeholder: on_bg(COLOR_SCHEME.placeholder),
        error: on_bg(COLOR_SCHEME.error),
        success: on_bg(COLOR_SCHEME.success),
        ..InputFieldStyle::default()
    }
}

/// The scheme's selection style on [`BG`].
pub fn selection_style() -> SelectionStyle {
    SelectionStyle {
        focused: on_bg(COLOR_SCHEME.hi),
        border: on_bg(COLOR_SCHEME.hi),
        general: base(),
        rows: [base(), base()],
    }
}

/// The scheme's tab line style on [`BG`].
pub fn scrolling_tabs_style() -> ScrollingTabsStyle {
    ScrollingTabsStyle {
        general: on_bg(COLOR_SCHEME.hi).bold(),
        ..ScrollingTabsStyle::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// TU-R-058 — every widget style that the scheme paints on its background is moved to `BG`.
    fn ut_widget_styles_use_the_background() {
        let input = input_field_style();
        for style in [
            input.general,
            input.focused,
            input.border,
            input.placeholder,
            input.error,
            input.success,
        ] {
            assert_eq!(style.bg, Some(BG), "{style:?}");
        }
        assert_eq!(input.cursor.bg, Some(COLOR_SCHEME.hi));
        assert_eq!(input.selection.bg, Some(COLOR_SCHEME.hi_bg));
        let selection = selection_style();
        for style in [
            selection.focused,
            selection.border,
            selection.general,
            selection.rows[0],
            selection.rows[1],
        ] {
            assert_eq!(style.bg, Some(BG), "{style:?}");
        }
        let tabs = scrolling_tabs_style();
        assert_eq!(tabs.general.bg, Some(BG));
        assert_eq!(tabs.selected.bg, Some(COLOR_SCHEME.hi_bg));
        assert_eq!(base().bg, Some(BG));
        assert_eq!(on_bg(COLOR_SCHEME.hi).fg, Some(COLOR_SCHEME.hi));
    }
}
