//! The main view's `:` command line, built on ferrowl-ui's `CommandLine` widget:
//! the prompt and help box while open, else the error, notice or hint bar.

use ferrowl_ui::state::{CommandLineState, CommandLineStateBuilder, InputFieldStateBuilder};
use ferrowl_ui::style::InputFieldStyle;
use ferrowl_ui::widgets::{CommandLine, CommandLineBuilder};

use crate::command::HELP;
use crate::view::theme;

const HINT: &str = ":  command  |  C-t+j C-t+k  tabs";

/// The state with the hint bar set, closed and empty.
pub fn state() -> CommandLineState {
    CommandLineStateBuilder::default()
        .hint(HINT.to_string())
        // An empty placeholder keeps the widget's "Enter value.." hint off the line.
        .input(
            InputFieldStateBuilder::default()
                .placeholder(Some(String::new()))
                .build()
                .expect("InputFieldStateBuilder fields all default"),
        )
        .build()
        .expect("CommandLineStateBuilder fields all default")
}

/// The widget, styled on the background, listing every command in its help box.
pub fn widget() -> CommandLine {
    CommandLineBuilder::default()
        .style(InputFieldStyle {
            general: theme::base(),
            focused: theme::base(),
            ..theme::input_field_style()
        })
        .highlight_style(theme::on_bg(theme::TEMPLATE.hi))
        .error_style(theme::on_bg(theme::TEMPLATE.error))
        .help(
            HELP.iter()
                .map(|(usage, desc)| (usage.to_string(), desc.to_string()))
                .collect::<Vec<_>>(),
        )
        .build()
        .expect("CommandLineBuilder fields all default")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use ferrowl_ui::state::CommandLineOutcome;
    use ratatui::layout::Rect;
    use ratatui::widgets::StatefulWidget;

    fn type_str(s: &mut CommandLineState, text: &str) {
        for c in text.chars() {
            assert_eq!(
                s.handle_key(KeyModifiers::NONE, KeyCode::Char(c)),
                Some(CommandLineOutcome::Consumed)
            );
        }
    }

    fn render_bottom(s: &mut CommandLineState, width: u16, height: u16) -> Vec<String> {
        crate::testkit::render_rows(width, height, |f| {
            let area = f.area();
            let bottom = Rect::new(0, area.height - 1, area.width, 1);
            StatefulWidget::render(&widget(), bottom, f.buffer_mut(), s);
        })
    }

    #[test]
    /// TU-R-025 — opening yields an empty, open line.
    fn ut_open_clears_input() {
        let mut s = state();
        assert!(!s.is_open());
        s.open();
        type_str(&mut s, "abc");
        s.open();
        assert!(s.is_open());
        assert_eq!(s.input().input(), "");
    }

    #[test]
    /// TU-R-027 — Enter submits the trimmed input and closes the line.
    fn ut_enter_submits_and_closes() {
        let mut s = state();
        s.open();
        type_str(&mut s, " zzz ");
        assert_eq!(
            s.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            Some(CommandLineOutcome::Submit("zzz".into()))
        );
        assert!(!s.is_open());
    }

    #[test]
    /// TU-R-028 — Esc closes without submitting.
    fn ut_esc_cancels() {
        let mut s = state();
        s.open();
        type_str(&mut s, "q");
        assert_eq!(
            s.handle_key(KeyModifiers::NONE, KeyCode::Esc),
            Some(CommandLineOutcome::Cancel)
        );
        assert!(!s.is_open());
    }

    #[test]
    /// TU-R-025, TU-R-034, TU-R-048 — the bottom line shows the prompt while open, else the error, else the hint.
    fn ut_render_prompt_and_error() {
        let mut s = state();
        s.open();
        type_str(&mut s, "wr");
        let rows = render_bottom(&mut s, 20, 1);
        assert_eq!(rows[0], ":wr");
        s.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        s.set_error(Some("unknown command: x".into()));
        let rows = render_bottom(&mut s, 20, 1);
        assert_eq!(rows[0], "unknown command: x");
        s.set_error(None);
        let rows = render_bottom(&mut s, 60, 1);
        assert_eq!(rows[0].trim(), HINT.trim());
    }

    #[test]
    /// TU-R-040 — a notice shows while closed and outlives an error clear until cleared itself.
    fn ut_notice_shows_until_cleared() {
        let mut s = state();
        s.set_notice(Some("loading projects…".into()));
        let rows = render_bottom(&mut s, 40, 1);
        assert_eq!(rows[0], "loading projects…");
        s.set_error(None);
        let rows = render_bottom(&mut s, 40, 1);
        assert_eq!(rows[0], "loading projects…");
        s.set_notice(None);
        let rows = render_bottom(&mut s, 60, 1);
        assert!(rows[0].contains("command"));
    }

    #[test]
    /// TU-R-026 — the help box lists every command above the prompt while open, and only then.
    fn ut_help_box_above_prompt_while_open() {
        let mut s = state();
        let closed = render_bottom(&mut s, 70, 12);
        assert!(!closed.join("\n").contains("quit"));
        s.open();
        let rows = render_bottom(&mut s, 70, 12);
        let joined = rows.join("\n");
        for (usage, desc) in crate::command::HELP {
            assert!(
                joined.contains(usage) && joined.contains(desc),
                "missing {usage}:\n{joined}"
            );
        }
        assert_eq!(rows[11], ":");
        assert!(rows[10].starts_with('└'), "{}", rows[10]);
    }
}
