//! The `:` prompt on the bottom line, with command-name completion and the error slot.

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::state::{InputFieldStateBuilder, SuggestInputState, SuggestInputStateBuilder};
use ferrowl_ui::style::InputFieldStyle;
use ferrowl_ui::traits::{HandleEvents, SetFocus};
use ferrowl_ui::widgets::{InputFieldBuilder, SuggestInput, SuggestInputBuilder, Widget};
use ferrowl_ui::{Border, COLOR_SCHEME, EventResult};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Paragraph, StatefulWidget, Widget as RenderWidget};

use crate::command::CommandProvider;

type Input = Widget<SuggestInputState<CommandProvider>, SuggestInput<String, CommandProvider>>;

/// What a key did to the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLineEvent {
    Consumed,
    /// Enter: the raw input, to be parsed by the caller. The line is closed.
    Submit(String),
    /// Esc: closed without executing.
    Cancel,
}

pub struct CommandLine {
    input: Input,
    open: bool,
    error: Option<String>,
}

impl Default for CommandLine {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandLine {
    pub fn new() -> CommandLine {
        let base = Style::default().fg(COLOR_SCHEME.text).bg(COLOR_SCHEME.bg);
        let state = SuggestInputStateBuilder::default()
            .provider(CommandProvider)
            .field(
                InputFieldStateBuilder::default()
                    .focused(false)
                    // An empty placeholder keeps the widget's "Enter value.." hint off the line.
                    .placeholder(Some(String::new()))
                    .build()
                    .expect("InputFieldStateBuilder fields all default"),
            )
            .build()
            .expect("provider is set");
        let widget = SuggestInputBuilder::default()
            .input_field(
                InputFieldBuilder::default()
                    .border(Border::None)
                    .margin(Margin::new(0, 0))
                    .style(InputFieldStyle {
                        general: base,
                        focused: base,
                        ..InputFieldStyle::default()
                    })
                    .build()
                    .expect("InputFieldBuilder fields all default"),
            )
            .build()
            .expect("SuggestInputBuilder fields all default");
        CommandLine {
            input: Widget { state, widget },
            open: false,
            error: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Clears the input and takes focus.
    pub fn open(&mut self) {
        self.input.state.set_input(String::new());
        self.input.state.set_cursor(0);
        SetFocus::set_focused(&mut self.input, true);
        self.open = true;
    }

    fn close(&mut self) {
        SetFocus::set_focused(&mut self.input, false);
        self.open = false;
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn set_error(&mut self, message: String) {
        self.error = Some(message);
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    #[cfg(test)]
    pub fn input(&self) -> &str {
        self.input.state.input()
    }

    #[cfg(test)]
    pub fn suggestions_open(&self) -> bool {
        self.input.state.suggestions_open()
    }

    /// Only meaningful while open.
    pub fn handle_key(&mut self, modifiers: KeyModifiers, code: KeyCode) -> CommandLineEvent {
        match self.input.handle_events(modifiers, code) {
            EventResult::Consumed => CommandLineEvent::Consumed,
            EventResult::Unhandled(_, KeyCode::Enter) => {
                let text = self.input.state.input().clone();
                self.close();
                CommandLineEvent::Submit(text)
            }
            EventResult::Unhandled(_, KeyCode::Esc) => {
                self.close();
                CommandLineEvent::Cancel
            }
            EventResult::Unhandled(..) => CommandLineEvent::Consumed,
        }
    }

    /// The bottom line: the prompt while open, else the error if any, else blank.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let base = Style::default().fg(COLOR_SCHEME.text).bg(COLOR_SCHEME.bg);
        buf.set_style(area, base);
        if self.open {
            let [prompt, rest] =
                Layout::horizontal([Constraint::Length(1), Constraint::Min(1)]).areas(area);
            Paragraph::new(":").style(base).render(prompt, buf);
            StatefulWidget::render(&self.input.widget, rest, buf, &mut self.input.state);
        } else if let Some(error) = self.error() {
            Paragraph::new(error)
                .style(Style::default().fg(COLOR_SCHEME.error).bg(COLOR_SCHEME.bg))
                .render(area, buf);
        }
    }

    /// The completion popup; call after every other widget of the frame.
    pub fn render_overlay(&mut self, bounds: Rect, buf: &mut Buffer) {
        if self.open {
            self.input
                .widget
                .render_overlay(bounds, buf, &mut self.input.state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_str(cl: &mut CommandLine, s: &str) {
        for c in s.chars() {
            assert_eq!(
                cl.handle_key(KeyModifiers::NONE, KeyCode::Char(c)),
                CommandLineEvent::Consumed
            );
        }
    }

    #[test]
    /// TU-R-025 — opening yields an empty, open line.
    fn ut_open_clears_input() {
        let mut cl = CommandLine::new();
        assert!(!cl.is_open());
        cl.open();
        type_str(&mut cl, "abc");
        cl.open();
        assert!(cl.is_open());
        assert_eq!(cl.input(), "");
    }

    #[test]
    /// TU-R-026 — typing a prefix opens completion; Enter first accepts the suggestion.
    fn ut_typing_prefix_offers_suggestions() {
        let mut cl = CommandLine::new();
        cl.open();
        type_str(&mut cl, "co");
        assert!(cl.suggestions_open());
        assert_eq!(
            cl.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            CommandLineEvent::Consumed
        );
        assert_eq!(cl.input(), "config");
        assert_eq!(
            cl.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            CommandLineEvent::Submit("config".into())
        );
        assert!(!cl.is_open());
    }

    #[test]
    /// TU-R-027 — Enter submits the raw input and closes the line.
    fn ut_enter_submits_and_closes() {
        let mut cl = CommandLine::new();
        cl.open();
        type_str(&mut cl, "zzz");
        assert_eq!(
            cl.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            CommandLineEvent::Submit("zzz".into())
        );
        assert!(!cl.is_open());
    }

    #[test]
    /// TU-R-028 — Esc closes without submitting.
    fn ut_esc_cancels() {
        let mut cl = CommandLine::new();
        cl.open();
        type_str(&mut cl, "q");
        assert!(cl.suggestions_open());
        assert_eq!(
            cl.handle_key(KeyModifiers::NONE, KeyCode::Esc),
            CommandLineEvent::Consumed
        );
        assert_eq!(
            cl.handle_key(KeyModifiers::NONE, KeyCode::Esc),
            CommandLineEvent::Cancel
        );
        assert!(!cl.is_open());
    }

    #[test]
    /// TU-R-025, TU-R-034 — the bottom line shows the prompt while open, the error when closed.
    fn ut_render_prompt_and_error() {
        let mut cl = CommandLine::new();
        cl.open();
        type_str(&mut cl, "wr");
        let rows = crate::testkit::render_rows(20, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert_eq!(rows[0], ":wr");
        cl.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        cl.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        cl.set_error("unknown command: x".into());
        let rows = crate::testkit::render_rows(20, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert_eq!(rows[0], "unknown command: x");
        cl.clear_error();
        let rows = crate::testkit::render_rows(20, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert_eq!(rows[0], "");
    }

    #[test]
    /// TU-R-026 — the completion popup draws above the prompt after the frame.
    fn ut_render_overlay_shows_suggestions() {
        let mut cl = CommandLine::new();
        cl.open();
        type_str(&mut cl, "w");
        let rows = crate::testkit::render_rows(20, 6, |f| {
            let area = f.area();
            let bottom = Rect::new(0, area.height - 1, area.width, 1);
            cl.render(bottom, f.buffer_mut());
            cl.render_overlay(area, f.buffer_mut());
        });
        let joined = rows.join("\n");
        assert!(joined.contains("wr"), "{joined}");
        assert_eq!(rows[5], ":w");
    }
}
