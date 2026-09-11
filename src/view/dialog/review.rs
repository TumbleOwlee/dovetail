//! Review mode of the pull request overlay's `Files Changed` tab: local draft comments,
//! remote threads, and the comment panel under the diff.

use std::ops::RangeInclusive;

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::state::{
    MarkdownInputFieldState, MarkdownInputFieldStateBuilder, MarkedRange, Side, TabBarState,
    VimMode,
};
use ferrowl_ui::traits::{HandleEvents, SetFocus};
use ferrowl_ui::widgets::{MarkdownInputFieldBuilder, TabBarBuilder};
use ferrowl_ui::{Border, EventResult};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::widgets::StatefulWidget;

use crate::github::pull::{ReviewThread, ThreadComment};
use crate::github::review::{Reply, Side as ApiSide, Thread as ApiThread};
use crate::view::theme;

/// One comment thread shown on the diff.
#[derive(Debug, Clone, PartialEq)]
pub enum UiThread {
    /// A new local comment, editable until submitted.
    Draft {
        path: String,
        side: Side,
        lines: RangeInclusive<usize>,
        body: String,
    },
    /// A thread loaded from GitHub, read-only, with an optional pending local reply.
    Remote {
        id: String,
        path: String,
        side: Side,
        lines: RangeInclusive<usize>,
        comments: Vec<ThreadComment>,
        reply: Option<String>,
    },
}

impl UiThread {
    fn path(&self) -> &str {
        match self {
            UiThread::Draft { path, .. } | UiThread::Remote { path, .. } => path,
        }
    }

    fn side(&self) -> Side {
        match self {
            UiThread::Draft { side, .. } | UiThread::Remote { side, .. } => *side,
        }
    }

    fn lines(&self) -> &RangeInclusive<usize> {
        match self {
            UiThread::Draft { lines, .. } | UiThread::Remote { lines, .. } => lines,
        }
    }

    /// The gutter mark color: purple for a local draft or pending reply, yellow for a
    /// remote thread.
    fn color(&self) -> ratatui::style::Color {
        match self {
            UiThread::Draft { .. } | UiThread::Remote { reply: Some(_), .. } => {
                theme::TEMPLATE.review
            }
            UiThread::Remote { reply: None, .. } => theme::TEMPLATE.warning,
        }
    }

    /// The tab bar caption: `draft` for a local draft, the first comment's author for a
    /// remote thread.
    fn caption(&self) -> String {
        match self {
            UiThread::Draft { .. } => "draft".to_string(),
            UiThread::Remote { comments, .. } => comments
                .first()
                .map(|c| format!("@{}", c.author.as_deref().unwrap_or("ghost")))
                .unwrap_or_else(|| "thread".to_string()),
        }
    }

    /// The read-only markdown shown for the thread: each comment as an `@author` heading
    /// with its body, a pending reply under a `pending reply` caption.
    fn rendered(&self) -> String {
        match self {
            UiThread::Draft { body, .. } => body.clone(),
            UiThread::Remote {
                comments, reply, ..
            } => {
                let mut parts: Vec<String> = comments
                    .iter()
                    .map(|c| {
                        format!(
                            "**@{}**\n\n{}",
                            c.author.as_deref().unwrap_or("ghost"),
                            c.body
                        )
                    })
                    .collect();
                if let Some(reply) = reply {
                    parts.push(format!("**pending reply**\n\n{reply}"));
                }
                parts.join("\n\n---\n\n")
            }
        }
    }
}

/// What the panel asks the caller to do after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelEvent {
    Consumed,
    /// The key was not for the panel; the focus belongs back on the diff.
    ToDiff,
}

/// The review state of the `Files Changed` tab: mode, threads, the open editor and the
/// comment panel.
pub struct ReviewPanel {
    pub active: bool,
    /// A submit request is running; local changes are locked until its outcome.
    pub submitting: bool,
    /// A review created by a failed submit, continued or discarded later.
    pub review_id: Option<String>,
    threads: Vec<UiThread>,
    /// The thread an editor is open for, with the editor state.
    editor: Option<(usize, MarkdownInputFieldState)>,
    /// The thread the read-only view shows, with its state.
    shown: Option<(usize, MarkdownInputFieldState)>,
    /// Index into the visible threads of the shown tab.
    tab: usize,
    notice: Option<String>,
}

fn field(read_only: bool, content: &str, focused: bool) -> MarkdownInputFieldState {
    let mut state = MarkdownInputFieldStateBuilder::default()
        .build()
        .expect("MarkdownInputFieldState fields all default");
    state.set_content(content);
    state.set_read_only(read_only);
    state.set_focused(focused);
    state
}

impl ReviewPanel {
    /// Unresolved remote threads shown, resolved ones dropped; review mode off.
    pub fn new(remote: &[ReviewThread]) -> ReviewPanel {
        let threads = remote
            .iter()
            .filter(|t| !t.resolved)
            .map(|t| UiThread::Remote {
                id: t.id.clone(),
                path: t.path.clone(),
                side: match t.side {
                    ApiSide::Left => Side::Old,
                    ApiSide::Right => Side::New,
                },
                lines: t.start_line as usize..=t.line as usize,
                comments: t.comments.clone(),
                reply: None,
            })
            .collect();
        ReviewPanel {
            active: false,
            submitting: false,
            review_id: None,
            threads,
            editor: None,
            shown: None,
            tab: 0,
            notice: None,
        }
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn set_notice(&mut self, notice: &str) {
        self.notice = Some(notice.to_string());
    }

    /// The gutter marks of every thread on `path`.
    pub fn marks(&self, path: &str) -> Vec<MarkedRange> {
        self.threads
            .iter()
            .filter(|t| t.path() == path)
            .map(|t| MarkedRange {
                side: t.side(),
                lines: t.lines().clone(),
                color: t.color(),
            })
            .collect()
    }

    /// Indices of the threads on `path` touched by the selected `old`/`new` file lines.
    pub fn visible(&self, path: &str, old: &[usize], new: &[usize]) -> Vec<usize> {
        self.threads
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.path() == path && {
                    let touched = match t.side() {
                        Side::Old => old,
                        Side::New => new,
                    };
                    touched.iter().any(|l| t.lines().contains(l))
                }
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Whether the comment panel takes rows: an editor is open or a thread is touched.
    pub fn panel_shown(&self, visible: &[usize]) -> bool {
        self.editor.is_some() || !visible.is_empty()
    }

    /// The thread the panel currently shows: the editor's, else the visible tab's.
    fn current(&self, visible: &[usize]) -> Option<usize> {
        if let Some((thread, _)) = &self.editor {
            return Some(*thread);
        }
        visible
            .get(self.tab.min(visible.len().saturating_sub(1)))
            .copied()
    }

    /// Opens an editor for a new draft on the lines, in place of any open one (saved
    /// first by the caller's focus switch).
    pub fn open_draft(&mut self, path: &str, side: Side, lines: RangeInclusive<usize>) {
        self.threads.push(UiThread::Draft {
            path: path.to_string(),
            side,
            lines,
            body: String::new(),
        });
        self.editor = Some((self.threads.len() - 1, field(false, "", true)));
        self.shown = None;
    }

    /// Opens an editor for the current thread when review mode allows it: a draft's
    /// body, or a remote thread's pending reply.
    fn open_current(&mut self, visible: &[usize], reply_only: bool) {
        let Some(index) = self.current(visible) else {
            return;
        };
        let content = match &self.threads[index] {
            UiThread::Draft { .. } if reply_only => return,
            UiThread::Draft { body, .. } => body.clone(),
            UiThread::Remote { reply, .. } => reply.clone().unwrap_or_default(),
        };
        self.editor = Some((index, field(false, &content, true)));
        self.shown = None;
    }

    /// Saves an open editor into its thread: a blank draft is dropped, a blank reply
    /// cleared. Safe to call with no editor open.
    pub fn save_editor(&mut self) {
        let Some((index, state)) = self.editor.take() else {
            return;
        };
        let content = state.content().trim().to_string();
        match &mut self.threads[index] {
            UiThread::Draft { body, .. } => {
                if content.is_empty() {
                    self.threads.remove(index);
                } else {
                    *body = content;
                }
            }
            UiThread::Remote { reply, .. } => {
                *reply = (!content.is_empty()).then_some(content);
            }
        }
        self.shown = None;
    }

    /// Gives or takes the comment panel focus; entering review mode on a draft reopens
    /// its editor.
    pub fn set_focused(&mut self, focused: bool, visible: &[usize]) {
        if focused {
            if self.editor.is_none()
                && self.active
                && !self.submitting
                && let Some(index) = self.current(visible)
                && matches!(self.threads[index], UiThread::Draft { .. })
            {
                self.open_current(visible, false);
            }
        } else {
            self.save_editor();
        }
        if let Some((_, state)) = &mut self.editor {
            state.set_focused(focused);
        }
        if let Some((_, state)) = &mut self.shown {
            state.set_focused(focused);
        }
    }

    /// One key while the comment panel is focused.
    pub fn handle_key(
        &mut self,
        visible: &[usize],
        modifiers: KeyModifiers,
        code: KeyCode,
    ) -> PanelEvent {
        let editing_insert = self
            .editor
            .as_ref()
            .is_some_and(|(_, s)| s.vim_mode() == VimMode::Insert);
        match (modifiers, code) {
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c @ ('[' | ']')))
                if !editing_insert =>
            {
                self.save_editor();
                let len = self.visible_len_after_save(visible);
                if len > 1 {
                    self.tab = if c == ']' {
                        (self.tab + 1) % len
                    } else {
                        (self.tab + len - 1) % len
                    };
                    self.shown = None;
                }
                PanelEvent::Consumed
            }
            (KeyModifiers::NONE, KeyCode::Char('r')) if self.editor.is_none() => {
                if !self.active {
                    self.set_notice("no review: run :review");
                } else if self.submitting {
                    self.set_notice("review busy");
                } else {
                    self.open_current(visible, true);
                }
                PanelEvent::Consumed
            }
            (KeyModifiers::CONTROL, KeyCode::Char('d')) if self.editor.is_some() => {
                if let Some((_, state)) = &mut self.editor {
                    // Delete through the editor's own vim pipeline so `u` restores it.
                    for (m, k) in [
                        (KeyModifiers::NONE, KeyCode::Esc),
                        (KeyModifiers::NONE, KeyCode::Char('g')),
                        (KeyModifiers::NONE, KeyCode::Char('g')),
                        (KeyModifiers::SHIFT, KeyCode::Char('V')),
                        (KeyModifiers::SHIFT, KeyCode::Char('G')),
                        (KeyModifiers::NONE, KeyCode::Char('d')),
                    ] {
                        state.handle_events(m, k);
                    }
                }
                PanelEvent::Consumed
            }
            _ => {
                if let Some((_, state)) = &mut self.editor {
                    return match state.handle_events(modifiers, code) {
                        EventResult::Consumed => PanelEvent::Consumed,
                        EventResult::Unhandled(..) => PanelEvent::ToDiff,
                    };
                }
                if let Some((_, state)) = &mut self.shown {
                    return match state.handle_events(modifiers, code) {
                        EventResult::Consumed => PanelEvent::Consumed,
                        EventResult::Unhandled(..) => PanelEvent::ToDiff,
                    };
                }
                PanelEvent::ToDiff
            }
        }
    }

    /// The visible-thread count after a save possibly removed a blank draft: the caller's
    /// list may hold a stale index, so it is re-clamped rather than recomputed here.
    fn visible_len_after_save(&self, visible: &[usize]) -> usize {
        visible.iter().filter(|i| **i < self.threads.len()).count()
    }

    /// Every unsent draft as an API comment and every pending reply, in thread order.
    pub fn pending(&self) -> (Vec<ApiThread>, Vec<Reply>) {
        let mut comments = Vec::new();
        let mut replies = Vec::new();
        for thread in &self.threads {
            match thread {
                UiThread::Draft {
                    path,
                    side,
                    lines,
                    body,
                } => comments.push(ApiThread {
                    path: path.clone(),
                    side: match side {
                        Side::Old => ApiSide::Left,
                        Side::New => ApiSide::Right,
                    },
                    start_line: *lines.start() as u32,
                    line: *lines.end() as u32,
                    body: body.clone(),
                }),
                UiThread::Remote {
                    id,
                    reply: Some(reply),
                    ..
                } => replies.push(Reply {
                    thread_id: id.clone(),
                    body: reply.clone(),
                }),
                UiThread::Remote { reply: None, .. } => {}
            }
        }
        (comments, replies)
    }

    /// Drops the first `added` drafts and clears the first `replied` pending replies, in
    /// the order `pending` reported them: they are on the review now.
    pub fn drop_sent(&mut self, added: usize, replied: usize) {
        let mut added = added;
        let mut replied = replied;
        self.threads.retain_mut(|thread| match thread {
            UiThread::Draft { .. } if added > 0 => {
                added -= 1;
                false
            }
            UiThread::Remote {
                reply: reply @ Some(_),
                ..
            } if replied > 0 => {
                replied -= 1;
                *reply = None;
                true
            }
            _ => true,
        });
        self.editor = None;
        self.shown = None;
    }

    /// Drops every draft and pending reply.
    pub fn clear_local(&mut self) {
        self.threads.retain_mut(|thread| match thread {
            UiThread::Draft { .. } => false,
            UiThread::Remote { reply, .. } => {
                *reply = None;
                true
            }
        });
        self.editor = None;
        self.shown = None;
    }

    /// Renders the tab bar and the current thread into `area`.
    pub fn render(&mut self, visible: &[usize], focused: bool, area: Rect, buf: &mut Buffer) {
        let body = if visible.len() > 1 && self.editor.is_none() {
            let [bar, body] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
            let tabs = TabBarBuilder::<String>::default()
                .style(theme::tab_bar_style())
                .direction(Direction::Horizontal)
                .build()
                .expect("TabBarBuilder fields all default");
            let mut state = TabBarState {
                titles: visible
                    .iter()
                    .map(|i| format!(" {} ", self.threads[*i].caption()))
                    .collect(),
                active: self.tab.min(visible.len() - 1),
                offset: 0,
            };
            StatefulWidget::render(&tabs, bar, buf, &mut state);
            body
        } else {
            area
        };
        let Some(index) = self.current(visible) else {
            return;
        };
        let editing = self.editor.as_ref().is_some_and(|(t, _)| *t == index);
        let title = match (&self.threads[index], editing) {
            (UiThread::Draft { .. }, _) => " comment ",
            (UiThread::Remote { .. }, true) => " reply ",
            (UiThread::Remote { .. }, false) => " thread ",
        };
        let widget = MarkdownInputFieldBuilder::default()
            .border(Border::Full(Margin::new(1, 0)))
            .title(Some(title.into()))
            .style(theme::input_field_style())
            .build()
            .expect("MarkdownInputField fields all default");
        if editing {
            if let Some((_, state)) = &mut self.editor {
                StatefulWidget::render(&widget, body, buf, state);
            }
            return;
        }
        if self.shown.as_ref().is_none_or(|(t, _)| *t != index) {
            self.shown = Some((index, field(true, &self.threads[index].rendered(), focused)));
        }
        if let Some((_, state)) = &mut self.shown {
            StatefulWidget::render(&widget, body, buf, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{buffer_rows, render_buffer};

    fn remote(id: &str, start: u32, line: u32, resolved: bool) -> ReviewThread {
        ReviewThread {
            id: id.into(),
            path: "a.rs".into(),
            side: ApiSide::Right,
            start_line: start,
            line,
            resolved,
            outdated: false,
            comments: vec![ThreadComment {
                author: Some("octo".into()),
                body: "looks off".into(),
            }],
        }
    }

    fn typed(panel: &mut ReviewPanel, visible: &[usize], text: &str) {
        panel.handle_key(visible, KeyModifiers::NONE, KeyCode::Char('i'));
        for c in text.chars() {
            panel.handle_key(visible, KeyModifiers::NONE, KeyCode::Char(c));
        }
        panel.handle_key(visible, KeyModifiers::NONE, KeyCode::Esc);
    }

    #[test]
    /// TU-R-081 — resolved remote threads are hidden and the rest marked in the gutter, yellow for a remote thread, purple once a reply is pending or for a local draft; the diff selection's touched file lines pick the visible threads by path and side.
    fn ut_remote_threads_marks_and_visibility() {
        let mut panel = ReviewPanel::new(&[
            remote("T_1", 3, 5, false),
            remote("T_2", 8, 8, true),
            remote("T_3", 10, 12, false),
        ]);
        let marks = panel.marks("a.rs");
        assert_eq!(marks.len(), 2, "resolved thread hidden");
        assert_eq!(marks[0].lines, 3..=5);
        assert!(marks.iter().all(|m| m.color == theme::TEMPLATE.warning));
        assert_eq!(panel.marks("other.rs"), vec![]);
        assert_eq!(panel.visible("a.rs", &[], &[4]), vec![0]);
        assert_eq!(
            panel.visible("a.rs", &[4], &[]),
            Vec::<usize>::new(),
            "old side misses"
        );
        assert_eq!(panel.visible("a.rs", &[], &[5, 11]), vec![0, 1]);
        assert!(panel.panel_shown(&[0]));
        assert!(!panel.panel_shown(&[]));

        panel.active = true;
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        typed(&mut panel, &[0], "me too");
        panel.set_focused(false, &[0]);
        assert_eq!(
            panel.marks("a.rs")[0].color,
            theme::TEMPLATE.review,
            "pending reply purple"
        );
        panel.open_draft("a.rs", Side::New, 7..=7);
        typed(&mut panel, &[], "new");
        panel.save_editor();
        assert_eq!(
            panel.marks("a.rs")[2].color,
            theme::TEMPLATE.review,
            "draft purple"
        );
    }

    #[test]
    /// TU-R-082 — an open draft editor saves its non-blank content when the focus moves away and a blank draft is dropped; Ctrl+D deletes the whole text through the vim pipeline so `u` restores it.
    fn ut_draft_save_blank_drop_and_ctrl_d_undo() {
        let mut panel = ReviewPanel::new(&[]);
        panel.active = true;
        panel.open_draft("a.rs", Side::New, 3..=5);
        typed(&mut panel, &[], "first line");
        panel.set_focused(false, &[]);
        let (threads, replies) = panel.pending();
        assert_eq!(replies, vec![]);
        assert_eq!(
            threads,
            vec![ApiThread {
                path: "a.rs".into(),
                side: ApiSide::Right,
                start_line: 3,
                line: 5,
                body: "first line".into(),
            }]
        );

        panel.open_draft("a.rs", Side::Old, 9..=9);
        panel.set_focused(false, &[]);
        assert_eq!(panel.pending().0.len(), 1, "blank draft dropped");

        panel.set_focused(true, &[0]);
        assert!(panel.marks("a.rs").len() == 1);
        panel.handle_key(&[0], KeyModifiers::CONTROL, KeyCode::Char('d'));
        panel.save_editor();
        assert_eq!(panel.pending().0.len(), 0, "Ctrl+D emptied the draft");

        panel.open_draft("a.rs", Side::New, 3..=5);
        typed(&mut panel, &[], "kept");
        panel.handle_key(&[], KeyModifiers::CONTROL, KeyCode::Char('d'));
        panel.handle_key(&[], KeyModifiers::NONE, KeyCode::Char('u'));
        panel.save_editor();
        assert_eq!(
            panel.pending().0[0].body,
            "kept",
            "u restores the deleted text"
        );
    }

    #[test]
    /// TU-R-083, TU-E-052, TU-E-054 — `r` opens a reply editor only in an active, idle review and notices otherwise; a saved reply is reported in thread order and a reply left blank is cleared.
    fn ut_reply_gating_and_pending() {
        let mut panel = ReviewPanel::new(&[remote("T_1", 3, 5, false)]);
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        assert_eq!(panel.notice(), Some("no review: run :review"));
        panel.active = true;
        panel.submitting = true;
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        assert_eq!(panel.notice(), Some("review busy"));
        panel.submitting = false;
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        typed(&mut panel, &[0], "will fix");
        panel.set_focused(false, &[0]);
        assert_eq!(
            panel.pending().1,
            vec![Reply {
                thread_id: "T_1".into(),
                body: "will fix".into(),
            }]
        );
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        panel.handle_key(&[0], KeyModifiers::CONTROL, KeyCode::Char('d'));
        panel.set_focused(false, &[0]);
        assert_eq!(panel.pending().1, vec![], "blank reply cleared");
    }

    #[test]
    /// TU-R-084 — after a failed submit only the comments and replies already added are dropped, in the order they were reported, so a retry sends the rest; `discard` semantics drop every local change.
    fn ut_drop_sent_and_clear_local() {
        let mut panel = ReviewPanel::new(&[remote("T_1", 3, 5, false)]);
        panel.active = true;
        for body in ["one", "two"] {
            panel.open_draft("a.rs", Side::New, 1..=1);
            typed(&mut panel, &[], body);
            panel.save_editor();
        }
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        typed(&mut panel, &[0], "re");
        panel.save_editor();
        assert_eq!(panel.pending().0.len(), 2);
        assert_eq!(panel.pending().1.len(), 1);
        panel.drop_sent(1, 0);
        let (threads, replies) = panel.pending();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].body, "two", "the first sent draft is gone");
        assert_eq!(replies.len(), 1, "unsent reply kept");
        panel.drop_sent(1, 1);
        assert_eq!(panel.pending(), (vec![], vec![]));

        panel.open_draft("a.rs", Side::New, 2..=2);
        typed(&mut panel, &[], "x");
        panel.save_editor();
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        typed(&mut panel, &[0], "y");
        panel.save_editor();
        panel.clear_local();
        assert_eq!(panel.pending(), (vec![], vec![]));
        assert_eq!(panel.marks("a.rs").len(), 1, "the remote thread stays");
    }

    #[test]
    /// TU-R-081 — more than one touched thread renders a horizontal tab bar captioned `draft` or `@<author>`; `[` and `]` switch the shown thread outside Insert mode; a remote thread renders read-only with `@<author>` and a `pending reply` caption.
    fn ut_thread_tabs_and_readonly_render() {
        let mut panel = ReviewPanel::new(&[remote("T_1", 3, 5, false)]);
        panel.active = true;
        panel.open_draft("a.rs", Side::New, 4..=4);
        typed(&mut panel, &[], "mine");
        panel.save_editor();
        let visible = vec![0, 1];
        let buf = render_buffer(60, 8, |f| {
            panel.render(&visible, true, f.area(), f.buffer_mut());
        });
        let rows = buffer_rows(&buf);
        assert!(
            rows[0].contains("@octo") && rows[0].contains("draft"),
            "{rows:?}"
        );
        assert!(rows[1].contains(" thread "), "remote first: {rows:?}");
        assert!(rows.iter().any(|r| r.contains("@octo")), "{rows:?}");
        assert!(rows.iter().any(|r| r.contains("looks off")), "{rows:?}");

        panel.handle_key(&visible, KeyModifiers::NONE, KeyCode::Char(']'));
        let buf = render_buffer(60, 8, |f| {
            panel.render(&visible, true, f.area(), f.buffer_mut());
        });
        let rows = buffer_rows(&buf);
        assert!(rows[1].contains(" comment "), "draft tab shown: {rows:?}");
        assert!(rows.iter().any(|r| r.contains("mine")), "{rows:?}");
        panel.handle_key(&visible, KeyModifiers::NONE, KeyCode::Char('['));
        let buf = render_buffer(60, 8, |f| {
            panel.render(&visible, true, f.area(), f.buffer_mut());
        });
        assert!(
            buffer_rows(&buf)[1].contains(" thread "),
            "back on the remote thread"
        );

        panel.handle_key(&visible, KeyModifiers::NONE, KeyCode::Char('r'));
        typed(&mut panel, &visible, "soon");
        panel.save_editor();
        let buf = render_buffer(60, 12, |f| {
            panel.render(&visible, true, f.area(), f.buffer_mut());
        });
        let rows = buffer_rows(&buf);
        assert!(rows.iter().any(|r| r.contains("pending reply")), "{rows:?}");
        assert!(rows.iter().any(|r| r.contains("soon")), "{rows:?}");
    }
}
