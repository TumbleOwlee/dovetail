//! Palette overrides on top of ferrowl-ui's fixed color scheme.

use ferrowl_ui::COLOR_SCHEME;
use ferrowl_ui::style::{InputFieldStyle, ScrollingTabsStyle, SelectionStyle};
use ratatui::style::{Color, Style};

/// Background of every view, darker than the scheme's own.
pub const BG: Color = Color::Rgb(18, 18, 18);

/// Every color role the views use, so a color changes in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorTemplate {
    pub bg: Color,
    pub text: Color,
    pub text_hi: Color,
    pub hi: Color,
    pub hi_bg: Color,
    pub border: Color,
    pub placeholder: Color,
    pub error: Color,
    pub success: Color,
    /// Timeline box borders per entry type.
    pub timeline: TimelineColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineColors {
    pub comment: Color,
    pub closed: Color,
    pub merged: Color,
    pub reopened: Color,
    pub labeled: Color,
    pub assigned: Color,
    pub milestoned: Color,
    pub renamed: Color,
    pub review_requested: Color,
    pub approved: Color,
    pub changes_requested: Color,
    pub reviewed: Color,
    pub referenced: Color,
}

pub const TEMPLATE: ColorTemplate = ColorTemplate {
    bg: BG,
    text: COLOR_SCHEME.text,
    text_hi: COLOR_SCHEME.text_hi,
    hi: COLOR_SCHEME.hi,
    hi_bg: COLOR_SCHEME.hi_bg,
    border: COLOR_SCHEME.border,
    placeholder: COLOR_SCHEME.placeholder,
    error: COLOR_SCHEME.error,
    success: COLOR_SCHEME.success,
    timeline: TimelineColors {
        comment: COLOR_SCHEME.border,
        closed: COLOR_SCHEME.error,
        merged: COLOR_SCHEME.hi,
        reopened: COLOR_SCHEME.success,
        labeled: COLOR_SCHEME.warning,
        assigned: COLOR_SCHEME.info,
        milestoned: COLOR_SCHEME.info,
        renamed: COLOR_SCHEME.warning,
        review_requested: COLOR_SCHEME.info,
        approved: COLOR_SCHEME.success,
        changes_requested: COLOR_SCHEME.error,
        reviewed: COLOR_SCHEME.border,
        referenced: COLOR_SCHEME.placeholder,
    },
};

/// Plain text on the background.
pub fn base() -> Style {
    Style::default().fg(TEMPLATE.text).bg(BG)
}

/// `color` on the background.
pub fn on_bg(color: Color) -> Style {
    Style::default().fg(color).bg(BG)
}

/// The scheme's input field style on [`BG`].
pub fn input_field_style() -> InputFieldStyle {
    InputFieldStyle {
        general: base(),
        focused: on_bg(TEMPLATE.hi),
        border: on_bg(TEMPLATE.border),
        placeholder: on_bg(TEMPLATE.placeholder),
        error: on_bg(TEMPLATE.error),
        success: on_bg(TEMPLATE.success),
        ..InputFieldStyle::default()
    }
}

/// The scheme's selection style on [`BG`].
pub fn selection_style() -> SelectionStyle {
    SelectionStyle {
        focused: on_bg(TEMPLATE.hi),
        border: on_bg(TEMPLATE.hi),
        general: base(),
        rows: [base(), base()],
    }
}

/// The scheme's tab line style on [`BG`].
pub fn scrolling_tabs_style() -> ScrollingTabsStyle {
    ScrollingTabsStyle {
        general: on_bg(TEMPLATE.hi).bold(),
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
        assert_eq!(input.cursor.bg, Some(TEMPLATE.hi));
        assert_eq!(input.selection.bg, Some(TEMPLATE.hi_bg));
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
        assert_eq!(tabs.selected.bg, Some(TEMPLATE.hi_bg));
        assert_eq!(base().bg, Some(BG));
        assert_eq!(on_bg(TEMPLATE.hi).fg, Some(TEMPLATE.hi));
    }
}
