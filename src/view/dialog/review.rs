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
use ratatui::widgets::{Block, StatefulWidget, Widget as RenderWidget};

use crate::github::pull::{ReviewThread, ThreadComment};
use crate::github::review::{Reply, Side as ApiSide, Thread as ApiThread};
use crate::view::dialog::details::{markdown_rows, markdown_state, markdown_widget};
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

    /// The gutter mark color: the purple review colour for every thread.
    fn color(&self) -> ratatui::style::Color {
        theme::TEMPLATE.review
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

    /// The thread box's comment boxes: one `(title, markdown body)` per comment, the
    /// pending reply last unless an editor holds it.
    fn boxes(&self, with_reply: bool) -> Vec<(String, String)> {
        match self {
            UiThread::Draft { body, .. } => vec![(" comment ".to_string(), body.clone())],
            UiThread::Remote {
                comments, reply, ..
            } => {
                let mut boxes: Vec<(String, String)> = comments
                    .iter()
                    .map(|c| {
                        (
                            format!(" @{} ", c.author.as_deref().unwrap_or("ghost")),
                            c.body.clone(),
                        )
                    })
                    .collect();
                if with_reply && let Some(reply) = reply {
                    boxes.push((" pending reply ".to_string(), reply.clone()));
                }
                boxes
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
    /// The reply editor holds the panel focus instead of the thread box.
    reply_focused: bool,
    /// The thread box's row scroll, clamped by the render.
    scroll: usize,
    /// A leading `g` awaiting its second, for `gg`.
    pending_g: bool,
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

/// A focused, editable markdown editor prefilled with `content`, for a conversation
/// comment draft.
pub(crate) fn comment_field(content: &str) -> MarkdownInputFieldState {
    field(false, content, true)
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
            reply_focused: false,
            scroll: 0,
            pending_g: false,
            notice: None,
        }
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn set_notice(&mut self, notice: &str) {
        self.notice = Some(notice.to_string());
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
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
        self.reply_focused = matches!(self.threads[index], UiThread::Remote { .. });
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
        self.reply_focused = false;
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
            self.reply_focused = false;
        }
        let editing = focused && self.editor_focused(visible);
        if let Some((_, state)) = &mut self.editor {
            state.set_focused(editing);
        }
        if let Some((_, state)) = &mut self.shown {
            state.set_focused(focused);
        }
    }

    /// Whether the focused editor sits in vim Insert mode, so Tab must reach it
    /// as an indent instead of cycling the focus.
    pub fn editing_insert(&self, visible: &[usize]) -> bool {
        self.editor_focused(visible)
            && self
                .editor
                .as_ref()
                .is_some_and(|(_, s)| s.vim_mode() == VimMode::Insert)
    }

    /// Whether an open editor holds the panel focus: a draft's always does, a reply's
    /// only while the reply stop is focused.
    fn editor_focused(&self, visible: &[usize]) -> bool {
        match (&self.editor, self.current(visible)) {
            (Some(_), Some(index)) => {
                matches!(self.threads[index], UiThread::Draft { .. }) || self.reply_focused
            }
            _ => false,
        }
    }

    /// Whether the panel has a thread-box stop and a reply-editor stop.
    fn two_stops(&self, visible: &[usize]) -> bool {
        self.editor.is_some()
            && self
                .current(visible)
                .is_some_and(|i| matches!(self.threads[i], UiThread::Remote { .. }))
    }

    /// Whether the reply editor takes rows under the thread box.
    pub fn reply_open(&self, visible: &[usize]) -> bool {
        self.two_stops(visible)
    }

    /// Puts the panel focus on its first stop entering forward, its last entering
    /// backward.
    pub fn enter(&mut self, visible: &[usize], forward: bool) {
        self.reply_focused = self.two_stops(visible) && !forward;
    }

    /// Moves the focus to the panel's next stop; `false` when it leaves the panel.
    pub fn advance(&mut self, visible: &[usize], forward: bool) -> bool {
        if !self.two_stops(visible) {
            return false;
        }
        if forward && !self.reply_focused {
            self.reply_focused = true;
            true
        } else if !forward && self.reply_focused {
            self.reply_focused = false;
            true
        } else {
            false
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
            (KeyModifiers::CONTROL, KeyCode::Char('d')) if self.editor_focused(visible) => {
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
                if self.editor_focused(visible)
                    && let Some((_, state)) = &mut self.editor
                {
                    return match state.handle_events(modifiers, code) {
                        EventResult::Consumed => PanelEvent::Consumed,
                        EventResult::Unhandled(..) => PanelEvent::ToDiff,
                    };
                }
                if self
                    .current(visible)
                    .is_some_and(|i| matches!(self.threads[i], UiThread::Remote { .. }))
                {
                    return self.scroll_key(modifiers, code);
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

    /// `j`, `k`, `gg` and `G` on the focused thread box; the render clamps the offset.
    fn scroll_key(&mut self, modifiers: KeyModifiers, code: KeyCode) -> PanelEvent {
        let leading_g = std::mem::take(&mut self.pending_g);
        match (modifiers, code) {
            (KeyModifiers::NONE, KeyCode::Char('j') | KeyCode::Down) => {
                self.scroll = self.scroll.saturating_add(1);
                PanelEvent::Consumed
            }
            (KeyModifiers::NONE, KeyCode::Char('k') | KeyCode::Up) => {
                self.scroll = self.scroll.saturating_sub(1);
                PanelEvent::Consumed
            }
            (KeyModifiers::NONE, KeyCode::Char('g')) => {
                if leading_g {
                    self.scroll = 0;
                } else {
                    self.pending_g = true;
                }
                PanelEvent::Consumed
            }
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                self.scroll = usize::MAX;
                PanelEvent::Consumed
            }
            _ => PanelEvent::ToDiff,
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
        self.reply_focused = false;
        self.scroll = 0;
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
        self.reply_focused = false;
        self.scroll = 0;
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
        let remote = matches!(self.threads[index], UiThread::Remote { .. });
        let editor_widget = |title: &str| {
            MarkdownInputFieldBuilder::default()
                .border(Border::Full(Margin::new(1, 0)))
                .title(Some(title.into()))
                .style(theme::input_field_style())
                .build()
                .expect("MarkdownInputField fields all default")
        };
        if remote {
            let (thread_area, reply_area) = if editing {
                let [thread, reply] =
                    Layout::vertical([Constraint::Min(0), Constraint::Length(REPLY_HEIGHT)])
                        .areas(body);
                (thread, Some(reply))
            } else {
                (body, None)
            };
            self.render_thread_box(index, editing, focused, thread_area, buf);
            if let (Some(reply), Some((_, state))) = (reply_area, &mut self.editor) {
                StatefulWidget::render(&editor_widget(" reply "), reply, buf, state);
            }
            return;
        }
        if editing {
            if let Some((_, state)) = &mut self.editor {
                StatefulWidget::render(&editor_widget(" comment "), body, buf, state);
            }
            return;
        }
        if self.shown.as_ref().is_none_or(|(t, _)| *t != index) {
            let body_text = match &self.threads[index] {
                UiThread::Draft { body, .. } => body.clone(),
                UiThread::Remote { .. } => String::new(),
            };
            self.shown = Some((index, field(true, &body_text, focused)));
        }
        if let Some((_, state)) = &mut self.shown {
            StatefulWidget::render(&editor_widget(" comment "), body, buf, state);
        }
    }

    /// The scrollable thread box: one bordered box per comment, blitted through a scratch
    /// buffer so a box can straddle the viewport's edges.
    fn render_thread_box(
        &mut self,
        index: usize,
        editing: bool,
        focused: bool,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let color = if focused && !self.editor_focused_at(index) {
            theme::TEMPLATE.hi
        } else {
            theme::TEMPLATE.border
        };
        let block = Block::bordered()
            .style(theme::on_bg(color))
            .title(" thread ");
        let inner = block.inner(area).inner(Margin::new(1, 0));
        block.render(area, buf);
        if inner.width < 5 || inner.height == 0 {
            return;
        }
        let boxes = self.threads[index].boxes(!editing);
        let text_width = inner.width.saturating_sub(4).max(1);
        let heights: Vec<u16> = boxes
            .iter()
            .map(|(_, body)| markdown_rows(body, text_width) + 2)
            .collect();
        let total: usize = heights.iter().map(|h| *h as usize).sum();
        self.scroll = self.scroll.min(total.saturating_sub(inner.height as usize));
        let scratch_area = Rect::new(0, 0, inner.width, (total as u16).clamp(1, 512));
        let mut scratch = Buffer::empty(scratch_area);
        scratch.set_style(scratch_area, theme::base());
        let mut y = 0u16;
        for ((title, body), height) in boxes.iter().zip(&heights) {
            if y >= scratch_area.height {
                break;
            }
            let rect = Rect::new(0, y, inner.width, (*height).min(scratch_area.height - y));
            let comment = Block::bordered()
                .style(theme::on_bg(theme::TEMPLATE.border))
                .title(title.clone());
            let text = comment.inner(rect).inner(Margin::new(1, 0));
            comment.render(rect, &mut scratch);
            if text.width > 0 && text.height > 0 {
                let mut state = markdown_state(body, false);
                StatefulWidget::render(&markdown_widget(), text, &mut scratch, &mut state);
            }
            y += height;
        }
        for row in 0..inner.height {
            let src = row as usize + self.scroll;
            if src >= scratch_area.height as usize {
                break;
            }
            for col in 0..inner.width {
                buf[(inner.x + col, inner.y + row)] = scratch[(col, src as u16)].clone();
            }
        }
    }

    /// Like [`ReviewPanel::editor_focused`] with the current index already known.
    fn editor_focused_at(&self, index: usize) -> bool {
        self.editor.is_some()
            && (matches!(self.threads[index], UiThread::Draft { .. }) || self.reply_focused)
    }
}

/// Rows of the reply editor's box under the thread box, borders included.
const REPLY_HEIGHT: u16 = 8;

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
    /// TU-R-081 — resolved remote threads are hidden and the rest marked in the gutter in the purple review colour, remote threads and local drafts alike; the diff selection's touched file lines pick the visible threads by path and side.
    fn ut_remote_threads_marks_and_visibility() {
        let mut panel = ReviewPanel::new(&[
            remote("T_1", 3, 5, false),
            remote("T_2", 8, 8, true),
            remote("T_3", 10, 12, false),
        ]);
        let marks = panel.marks("a.rs");
        assert_eq!(marks.len(), 2, "resolved thread hidden");
        assert_eq!(marks[0].lines, 3..=5);
        assert!(marks.iter().all(|m| m.color == theme::TEMPLATE.review));
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
            "remote threads purple too"
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
    /// TU-R-081 — a remote thread renders as a thread box holding one bordered box per comment titled `@<author>`; `j`/`k` and `gg`/`G` scroll the focused thread box, clamped; a pending reply shows as a last box titled `pending reply` while no editor is open.
    fn ut_thread_box_scrolls() {
        let mut thread = remote("T_1", 3, 5, false);
        thread.comments = (0..6)
            .map(|i| ThreadComment {
                author: Some(format!("user{i}")),
                body: format!("comment number {i}"),
            })
            .collect();
        let mut panel = ReviewPanel::new(&[thread]);
        let draw = |panel: &mut ReviewPanel| {
            let buf = render_buffer(50, 10, |f| {
                panel.render(&[0], true, f.area(), f.buffer_mut());
            });
            buffer_rows(&buf)
        };
        let rows = draw(&mut panel);
        assert!(rows[0].contains(" thread "), "{rows:?}");
        assert!(
            rows.iter().any(|r| r.contains(" @user0 "))
                && rows.iter().any(|r| r.contains("comment number 0")),
            "author-titled comment boxes: {rows:?}"
        );
        assert!(
            !rows.iter().any(|r| r.contains("comment number 5")),
            "later comments below the fold: {rows:?}"
        );
        for _ in 0..40 {
            panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('j'));
        }
        let rows = draw(&mut panel);
        assert!(
            rows.iter().any(|r| r.contains("comment number 5")),
            "scrolled to the end, clamped: {rows:?}"
        );
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('g'));
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('g'));
        let rows = draw(&mut panel);
        assert!(
            rows.iter().any(|r| r.contains("comment number 0")),
            "gg back to the top: {rows:?}"
        );
        panel.handle_key(&[0], KeyModifiers::SHIFT, KeyCode::Char('G'));
        let rows = draw(&mut panel);
        assert!(
            rows.iter().any(|r| r.contains("comment number 5")),
            "G to the bottom: {rows:?}"
        );

        panel.active = true;
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        typed(&mut panel, &[0], "soon");
        panel.set_focused(false, &[0]);
        panel.handle_key(&[0], KeyModifiers::SHIFT, KeyCode::Char('G'));
        let rows = draw(&mut panel);
        assert!(
            rows.iter().any(|r| r.contains(" pending reply "))
                && rows.iter().any(|r| r.contains("soon")),
            "pending reply as the last box: {rows:?}"
        );
    }

    #[test]
    /// TU-R-083 — `r` opens the reply editor in its own `reply` box under the thread box, both visible, the editor focused; Tab stops: the panel reports two stops and moves between them.
    fn ut_reply_box_under_thread() {
        let mut panel = ReviewPanel::new(&[remote("T_1", 3, 5, false)]);
        panel.active = true;
        panel.set_focused(true, &[0]);
        panel.handle_key(&[0], KeyModifiers::NONE, KeyCode::Char('r'));
        assert!(panel.reply_open(&[0]));
        let buf = render_buffer(50, 14, |f| {
            panel.render(&[0], true, f.area(), f.buffer_mut());
        });
        let rows = buffer_rows(&buf);
        assert!(rows[0].contains(" thread "), "thread box stays: {rows:?}");
        assert!(rows.iter().any(|r| r.contains(" @octo ")), "{rows:?}");
        assert!(
            rows[6].contains(" reply "),
            "reply box of eight rows under the thread box: {rows:?}"
        );
        assert!(panel.advance(&[0], false), "reply back to the thread box");
        assert!(!panel.advance(&[0], false), "then out of the panel");
        assert!(panel.advance(&[0], true), "thread box forward to the reply");
        assert!(!panel.advance(&[0], true), "then out of the panel");
        typed(&mut panel, &[0], "on it");
        panel.set_focused(false, &[0]);
        assert_eq!(panel.pending().1[0].body, "on it");
        assert!(!panel.reply_open(&[0]), "saved editor closes the reply box");
    }

    #[test]
    /// TU-R-081 — more than one touched thread renders a horizontal tab bar captioned `draft` or `@<author>`; `[` and `]` switch the shown thread outside Insert mode.
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
    }
}
