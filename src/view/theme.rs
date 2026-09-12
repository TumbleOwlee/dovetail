//! Palette overrides on top of ferrowl-ui's fixed color scheme.

use ferrowl_ui::COLOR_SCHEME;
use ferrowl_ui::style::{
    DiffViewStyle, InputFieldStyle, MarkdownTheme, MarkdownThemeBuilder, SelectionStyle,
    SyntaxTheme, SyntaxThemeBuilder, TabBarStyle,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

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
    pub warning: Color,
    /// Review mode: the status label background and local-change gutter marks.
    pub review: Color,
    pub info: Color,
    pub row: [Color; 2],
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
    text: Color::White,
    text_hi: COLOR_SCHEME.text_hi,
    hi: COLOR_SCHEME.hi,
    hi_bg: COLOR_SCHEME.hi_bg,
    border: Color::White,
    placeholder: COLOR_SCHEME.placeholder,
    error: COLOR_SCHEME.error,
    success: COLOR_SCHEME.success,
    warning: COLOR_SCHEME.warning,
    review: Color::Rgb(72, 40, 116),
    info: COLOR_SCHEME.info,
    row: COLOR_SCHEME.row,
    timeline: TimelineColors {
        comment: Color::White,
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
        reviewed: Color::White,
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

/// The line on the highlight background, every span included.
pub fn highlighted(mut line: Line<'static>) -> Line<'static> {
    for span in &mut line.spans {
        span.style = span.style.bg(TEMPLATE.hi_bg);
    }
    line.patch_style(Style::new().bg(TEMPLATE.hi_bg))
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

/// The file tree's status styles on [`BG`]: added in the success color, removed in the
/// error color, modified in the text color.
pub fn status_theme() -> SyntaxTheme {
    SyntaxThemeBuilder::default()
        .added(on_bg(TEMPLATE.success))
        .removed(on_bg(TEMPLATE.error))
        .meta(base())
        .build()
        .expect("SyntaxTheme fields all default")
}

/// The scheme's diff widget style on [`BG`]; the added/removed bands keep the scheme's own.
pub fn diff_view_style() -> DiffViewStyle {
    DiffViewStyle {
        general: base(),
        focused: on_bg(TEMPLATE.hi),
        border: on_bg(TEMPLATE.border),
        selection: Style::default().fg(TEMPLATE.text_hi).bg(TEMPLATE.hi_bg),
        ..DiffViewStyle::default()
    }
}

/// The scheme's tab line style on [`BG`].
pub fn tab_bar_style() -> TabBarStyle {
    TabBarStyle {
        general: on_bg(TEMPLATE.hi).bold(),
        selected: Style::default().fg(TEMPLATE.text).bg(TEMPLATE.hi_bg).bold(),
    }
}

/// The table row styles: the template's text on the two row elevations.
pub fn table_rows() -> [Style; 2] {
    [
        Style::default().fg(TEMPLATE.text).bg(TEMPLATE.row[0]),
        Style::default().fg(TEMPLATE.text).bg(TEMPLATE.row[1]),
    ]
}

/// The markdown styles on the template: the headings and the quote body that the widget
/// would otherwise color from the scheme's own text color.
pub fn markdown_theme() -> MarkdownTheme {
    MarkdownThemeBuilder::default()
        .heading([
            Style::default()
                .fg(TEMPLATE.hi)
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(TEMPLATE.info)
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(TEMPLATE.success)
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(TEMPLATE.warning)
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(TEMPLATE.text_hi)
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(TEMPLATE.text)
                .add_modifier(Modifier::BOLD),
        ])
        .quote_text(
            Style::default()
                .fg(TEMPLATE.text)
                .add_modifier(Modifier::DIM | Modifier::ITALIC),
        )
        .build()
        .expect("MarkdownThemeBuilder fields all default")
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
        let status = status_theme();
        for style in [status.added, status.removed, status.meta] {
            assert_eq!(style.bg, Some(BG), "{style:?}");
        }
        let diff = diff_view_style();
        for style in [diff.general, diff.focused, diff.border] {
            assert_eq!(style.bg, Some(BG), "{style:?}");
        }
        let tabs = tab_bar_style();
        assert_eq!(tabs.general.bg, Some(BG));
        assert_eq!(tabs.selected.bg, Some(TEMPLATE.hi_bg));
        assert_eq!(base().bg, Some(BG));
        assert_eq!(on_bg(TEMPLATE.hi).fg, Some(TEMPLATE.hi));
    }

    #[test]
    /// TU-R-088 — the default box border is white, the timeline comment and review entries with it.
    fn ut_default_border_is_white() {
        assert_eq!(TEMPLATE.border, Color::White);
        assert_eq!(TEMPLATE.timeline.comment, Color::White);
        assert_eq!(TEMPLATE.timeline.reviewed, Color::White);
    }

    #[test]
    /// TU-R-074, TU-R-100 — the template's text color, and every style built from it, is white, the file tree's modified marker included.
    fn ut_text_color_is_white() {
        assert_eq!(TEMPLATE.text, Color::White);
        assert_eq!(base().fg, Some(Color::White));
        assert_eq!(status_theme().meta.fg, Some(Color::White));
    }

    #[test]
    /// TU-R-100, TU-R-101 — every widget style field that carries plain text takes the template's text color, and the fields carrying a role color of their own keep it.
    fn ut_widget_text_styles_use_the_template_text() {
        for fg in [
            input_field_style().general.fg,
            selection_style().general.fg,
            selection_style().rows[0].fg,
            selection_style().rows[1].fg,
            status_theme().meta.fg,
            diff_view_style().general.fg,
            tab_bar_style().selected.fg,
            table_rows()[0].fg,
            table_rows()[1].fg,
            markdown_theme().quote_text.fg,
            markdown_theme().heading(6).fg,
        ] {
            assert_eq!(fg, Some(Color::White));
        }
        assert_eq!(input_field_style().cursor.fg, Some(TEMPLATE.text_hi));
        assert_eq!(
            diff_view_style().selection,
            Style::default().fg(TEMPLATE.text_hi).bg(TEMPLATE.hi_bg)
        );
        assert_eq!(tab_bar_style().selected.bg, Some(TEMPLATE.hi_bg));
    }
}
