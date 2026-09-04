//! The `:` prompt on the bottom line, the help box above it while open, and the hint bar,
//! error or notice shown there while closed.

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::state::{InputFieldState, InputFieldStateBuilder};
use ferrowl_ui::style::InputFieldStyle;
use ferrowl_ui::traits::{HandleEvents, SetFocus};
use ferrowl_ui::widgets::{InputField, InputFieldBuilder, Widget};
use ferrowl_ui::{Border, COLOR_SCHEME, EventResult};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, StatefulWidget, Widget as RenderWidget};

use crate::command::HELP;

type Input = Widget<InputFieldState, InputField<String>>;

const HINT: &str = ":  command  |  C-t+h C-t+l  tabs";
const HELP_WIDTH: u16 = 62;
const USAGE_COLUMN: usize = 12;

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
    /// Shown until the next key press.
    error: Option<String>,
    /// Shown until explicitly cleared; outranked by an error.
    notice: Option<String>,
}

impl Default for CommandLine {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandLine {
    pub fn new() -> CommandLine {
        let base = Style::default().fg(COLOR_SCHEME.text).bg(COLOR_SCHEME.bg);
        let state = InputFieldStateBuilder::default()
            .focused(false)
            // An empty placeholder keeps the widget's "Enter value.." hint off the line.
            .placeholder(Some(String::new()))
            .build()
            .expect("InputFieldStateBuilder fields all default");
        let widget = InputFieldBuilder::default()
            .border(Border::None)
            .margin(Margin::new(0, 0))
            .style(InputFieldStyle {
                general: base,
                focused: base,
                ..InputFieldStyle::default()
            })
            .build()
            .expect("InputFieldBuilder fields all default");
        CommandLine {
            input: Widget { state, widget },
            open: false,
            error: None,
            notice: None,
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

    pub fn set_notice(&mut self, message: String) {
        self.notice = Some(message);
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    #[cfg(test)]
    pub fn input(&self) -> &str {
        self.input.state.input()
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

    /// The bottom line: the prompt while open; else the error, the notice, or the hint bar.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let base = Style::default().fg(COLOR_SCHEME.text).bg(COLOR_SCHEME.bg);
        buf.set_style(area, base);
        if self.open {
            let [prompt, rest] =
                Layout::horizontal([Constraint::Length(1), Constraint::Min(1)]).areas(area);
            Paragraph::new(":")
                .style(Style::default().fg(COLOR_SCHEME.hi).bg(COLOR_SCHEME.bg))
                .render(prompt, buf);
            StatefulWidget::render(&self.input.widget, rest, buf, &mut self.input.state);
        } else if let Some(error) = self.error() {
            Paragraph::new(error)
                .style(Style::default().fg(COLOR_SCHEME.error).bg(COLOR_SCHEME.bg))
                .render(area, buf);
        } else if let Some(notice) = &self.notice {
            Paragraph::new(notice.as_str())
                .style(base)
                .render(area, buf);
        } else {
            Paragraph::new(HINT).style(base).render(area, buf);
        }
    }

    /// The help box, anchored above the prompt; call after every other widget of the frame.
    /// `bounds` is the whole frame: the prompt is assumed to be its bottom line.
    pub fn render_overlay(&mut self, bounds: Rect, buf: &mut Buffer) {
        if !self.open || bounds.height < 3 {
            return;
        }
        let usage_style = Style::default()
            .fg(COLOR_SCHEME.hi)
            .bg(COLOR_SCHEME.bg)
            .bold();
        let desc_style = Style::default().fg(COLOR_SCHEME.text).bg(COLOR_SCHEME.bg);
        let lines: Vec<Line> = HELP
            .iter()
            .map(|(usage, desc)| {
                Line::from(vec![
                    Span::styled(format!("{usage:<USAGE_COLUMN$}"), usage_style),
                    Span::styled(*desc, desc_style),
                ])
            })
            .collect();
        let height = (lines.len() as u16 + 2).min(bounds.height - 1);
        let popup = Rect {
            x: bounds.x,
            y: bounds.y + bounds.height - 1 - height,
            width: HELP_WIDTH.min(bounds.width),
            height,
        };
        Clear.render(popup, buf);
        let block =
            Block::bordered().style(Style::default().fg(COLOR_SCHEME.hi).bg(COLOR_SCHEME.bg));
        let inner = block.inner(popup);
        block.render(popup, buf);
        Paragraph::new(lines)
            .style(Style::default().bg(COLOR_SCHEME.bg))
            .render(inner, buf);
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
        assert_eq!(
            cl.handle_key(KeyModifiers::NONE, KeyCode::Esc),
            CommandLineEvent::Cancel
        );
        assert!(!cl.is_open());
    }

    #[test]
    /// TU-R-025, TU-R-034, TU-R-048 — the bottom line shows the prompt while open, else the error, else the hint.
    fn ut_render_prompt_and_error() {
        let mut cl = CommandLine::new();
        cl.open();
        type_str(&mut cl, "wr");
        let rows = crate::testkit::render_rows(20, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert_eq!(rows[0], ":wr");
        cl.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        cl.set_error("unknown command: x".into());
        let rows = crate::testkit::render_rows(20, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert_eq!(rows[0], "unknown command: x");
        cl.clear_error();
        let rows = crate::testkit::render_rows(60, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert_eq!(rows[0].trim(), ":  command  |  C-t+h C-t+l  tabs");
    }

    #[test]
    /// TU-R-040 — a notice shows while closed and outlives keys until cleared.
    fn ut_notice_shows_until_cleared() {
        let mut cl = CommandLine::new();
        cl.set_notice("loading projects…".into());
        let rows = crate::testkit::render_rows(40, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert_eq!(rows[0], "loading projects…");
        cl.clear_error();
        let rows = crate::testkit::render_rows(40, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert_eq!(rows[0], "loading projects…");
        cl.clear_notice();
        let rows = crate::testkit::render_rows(60, 1, |f| cl.render(f.area(), f.buffer_mut()));
        assert!(rows[0].contains("command"));
    }

    #[test]
    /// TU-R-026 — the help box lists every command above the prompt while open, and only then.
    fn ut_help_box_above_prompt_while_open() {
        let mut cl = CommandLine::new();
        let closed = crate::testkit::render_rows(70, 12, |f| {
            let area = f.area();
            let bottom = Rect::new(0, area.height - 1, area.width, 1);
            cl.render(bottom, f.buffer_mut());
            cl.render_overlay(area, f.buffer_mut());
        });
        assert!(!closed.join("\n").contains("quit"));
        cl.open();
        let rows = crate::testkit::render_rows(70, 12, |f| {
            let area = f.area();
            let bottom = Rect::new(0, area.height - 1, area.width, 1);
            cl.render(bottom, f.buffer_mut());
            cl.render_overlay(area, f.buffer_mut());
        });
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
