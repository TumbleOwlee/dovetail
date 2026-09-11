//! Details overlay shared by issues and pull requests: a description card and one box per
//! timeline item at the left, scrolled together, and a bar of focusable boxes at the right.

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::state::{
    CodeInputFieldStateBuilder, CommandLineOutcome, CommandLineState, MarkdownInputFieldState,
    MarkdownInputFieldStateBuilder,
};
use ferrowl_ui::traits::{HandleEvents, SetFocus};
use ferrowl_ui::widgets::{CommandLineBuilder, MarkdownInputField, MarkdownInputFieldBuilder};
use ferrowl_ui::{Border, EventResult};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, HorizontalAlignment, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, StatefulWidget, Widget};

use crate::github::blob::Blob;
use crate::github::board::Label;
use crate::github::files::ChangedFile;
use crate::github::pull::{Commit, ReviewState, ReviewThread};
use crate::github::review::{ReviewAction, ReviewOutcome, ReviewResult, Verdict};
use crate::github::timeline::{Event, TimelineItem};
use crate::view::board::{badge_text_color, label_color};
use crate::view::dialog::commits::{CommitRequest, CommitsView};
use crate::view::dialog::files::FilesState;
use crate::view::{notice, tabs, theme};

/// Screen cells left free around the overlay on each side.
const INSET: Margin = Margin::new(1, 1);

/// Space between a card's borders and its text: two columns, one row.
const CARD_MARGIN: Margin = Margin::new(2, 1);

/// Space between a bar box's or an event box's borders and its lines: two columns, no rows.
const BOX_MARGIN: Margin = Margin::new(2, 0);

/// Columns of the bar at the overlay's right.
const BAR_WIDTH: u16 = 30;

/// What Enter on a bar entry opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Issue {
        /// GraphQL node id.
        id: String,
        number: u64,
        title: String,
    },
    Pull {
        owner: String,
        repo: String,
        number: u64,
        title: String,
    },
}

/// One box of the right bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarBox {
    pub title: &'static str,
    /// One entry per line; empty renders as `None`.
    pub lines: Vec<Line<'static>>,
    /// `links[i]` opens from `lines[i]`; empty when no entry opens anything.
    pub links: Vec<Link>,
}

/// The tabs of a pull request overlay; an issue has only the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Panes {
    Conversation,
    Pull {
        commits: Vec<Commit>,
        files: Vec<ChangedFile>,
        owner: String,
        repo: String,
        /// The head commit whose blobs the `Files Changed` tab shows.
        head_oid: String,
        /// GraphQL node id, the handle for review mutations.
        pull_id: String,
        /// Review comment threads on the diff.
        threads: Vec<ReviewThread>,
    },
}

/// A file's content to request: the path at the head commit of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRef {
    pub owner: String,
    pub repo: String,
    pub oid: String,
    pub path: String,
}

fn blob_ref(panes: &Panes, path: String) -> Option<BlobRef> {
    match panes {
        Panes::Conversation => None,
        Panes::Pull {
            owner,
            repo,
            head_oid,
            ..
        } => Some(BlobRef {
            owner: owner.clone(),
            repo: repo.clone(),
            oid: head_oid.clone(),
            path,
        }),
    }
}

/// The reference for a file's content at a commit of the pull request.
fn commit_blob_ref(panes: &Panes, sha: String, path: String) -> Option<BlobRef> {
    match panes {
        Panes::Conversation => None,
        Panes::Pull { owner, repo, .. } => Some(BlobRef {
            owner: owner.clone(),
            repo: repo.clone(),
            oid: sha,
            path,
        }),
    }
}

/// A pull request whose details to request anew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestRef {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

/// A commit whose changed files to request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRef {
    pub owner: String,
    pub repo: String,
    pub sha: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailsTab {
    Conversation,
    Commits,
    Files,
}

impl DetailsTab {
    pub const ALL: [DetailsTab; 3] = [
        DetailsTab::Conversation,
        DetailsTab::Commits,
        DetailsTab::Files,
    ];

    fn index(self) -> usize {
        DetailsTab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    /// Wraps at both ends.
    fn next(self) -> DetailsTab {
        DetailsTab::ALL[(self.index() + 1) % DetailsTab::ALL.len()]
    }

    /// Wraps at both ends.
    fn previous(self) -> DetailsTab {
        DetailsTab::ALL[(self.index() + DetailsTab::ALL.len() - 1) % DetailsTab::ALL.len()]
    }

    /// The tab line's caption, written down the line.
    fn label(self) -> &'static str {
        match self {
            DetailsTab::Conversation => "CONVERSATION",
            DetailsTab::Commits => "COMMITS",
            DetailsTab::Files => "FILES",
        }
    }
}

/// What the overlay shows once loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsContent {
    pub panes: Panes,
    /// GraphQL node id of the issue or pull request, the `addComment` subject.
    pub subject_id: String,
    pub title: String,
    /// `open`, `closed`, `merged` or `draft`.
    pub state: &'static str,
    /// `None` when the author account was deleted.
    pub author: Option<String>,
    pub body: String,
    pub timeline: Vec<TimelineItem>,
    pub boxes: Vec<SidebarBox>,
}

/// What the caller does after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailsEvent {
    Consumed,
    Close,
    /// The caller closes this overlay and opens the link.
    Open(Link),
    /// The caller queues the request for this file's content.
    Fetch(BlobRef),
    /// The caller queues the request for this commit's changed files.
    FetchCommit(CommitRef),
    /// The caller runs this review action.
    Review(ReviewAction),
    /// The caller posts this conversation comment.
    Comment {
        subject_id: String,
        body: String,
    },
}

/// What to request anew after a posted conversation comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentRefetch {
    Pull(PullRequestRef),
    Issue(String),
}

enum Content {
    Loading,
    Failed(String),
    Loaded {
        content: Box<DetailsContent>,
        scroll: usize,
        /// Index of the focused box.
        focus: usize,
        /// Entry the focused box's cursor rests on.
        cursor: usize,
        tab: DetailsTab,
        /// Ctrl+T was pressed; the next key selects a tab.
        prefix: bool,
        commits: Box<CommitsView>,
        files: Box<FilesState>,
        /// The overlay's own command line, opened with `:` on the `Files Changed` tab
        /// and the conversation views.
        command: Box<CommandLineState>,
        /// The conversation comment draft editor while its box is open.
        comment: Option<Box<MarkdownInputFieldState>>,
        /// The comment editor holds the conversation view's focus.
        comment_focused: bool,
        /// A comment post is running; the draft is locked until its outcome.
        posting: bool,
        /// A message popup outside review mode, dismissed by the next key.
        notice: Option<String>,
    },
}

pub struct DetailsDialog {
    number: u64,
    title: String,
    loading: &'static str,
    content: Content,
}

impl DetailsDialog {
    /// Opens in the loading state, showing `loading` until a result arrives.
    pub fn new(number: u64, title: String, loading: &'static str) -> DetailsDialog {
        DetailsDialog {
            number,
            title,
            loading,
            content: Content::Loading,
        }
    }

    /// Replaces the loading state with the content or the failure; a pull request's first
    /// changed file is returned for its content to be requested.
    pub fn set_result(&mut self, result: Result<DetailsContent, impl ToString>) -> Option<BlobRef> {
        self.content = match result {
            Ok(content) => Content::Loaded {
                commits: Box::new(CommitsView::new(match &content.panes {
                    Panes::Conversation => &[],
                    Panes::Pull { commits, .. } => commits,
                })),
                files: Box::new(match &content.panes {
                    Panes::Conversation => FilesState::new(&[]),
                    Panes::Pull { files, threads, .. } => FilesState::with_review(files, threads),
                }),
                content: Box::new(content),
                scroll: 0,
                focus: 0,
                cursor: 0,
                tab: DetailsTab::Conversation,
                prefix: false,
                command: Box::default(),
                comment: None,
                comment_focused: false,
                posting: false,
                notice: None,
            },
            Err(e) => Content::Failed(e.to_string()),
        };
        self.take_request()
    }

    /// Stores a file's content, routed by its commit id: the head commit's to the
    /// `Files Changed` tab, any other to that commit's diff. Returns the next file whose
    /// content is needed, if any.
    pub fn handle_blob(
        &mut self,
        oid: &str,
        path: &str,
        result: Result<Blob, impl ToString>,
    ) -> Option<BlobRef> {
        if let Content::Loaded {
            content,
            files,
            commits,
            ..
        } = &mut self.content
            && let Panes::Pull {
                files: changed,
                head_oid,
                ..
            } = &content.panes
        {
            if oid == head_oid {
                files.handle_blob(changed, path, result);
            } else {
                commits.handle_blob(oid, path, result);
            }
        }
        self.take_request()
    }

    /// Stores a commit's file list and returns the first file whose content is needed,
    /// if any.
    pub fn handle_commit(
        &mut self,
        sha: &str,
        result: Result<Vec<ChangedFile>, impl ToString>,
    ) -> Option<BlobRef> {
        if let Content::Loaded { commits, .. } = &mut self.content {
            commits.handle_commit(sha, result);
        }
        self.take_request()
    }

    fn take_request(&mut self) -> Option<BlobRef> {
        let Content::Loaded {
            content,
            files,
            commits,
            ..
        } = &mut self.content
        else {
            return None;
        };
        if let Some(path) = files.take_request() {
            return blob_ref(&content.panes, path);
        }
        match commits.take_request() {
            Some(CommitRequest::Blob { sha, path }) => commit_blob_ref(&content.panes, sha, path),
            // A file-list request only arises from Enter, whose key path maps it itself.
            Some(CommitRequest::Files { .. }) | None => None,
        }
    }

    pub fn handle_key(&mut self, modifiers: KeyModifiers, code: KeyCode) -> DetailsEvent {
        // An open message popup swallows the next key, whatever it is.
        if let Content::Loaded { notice, .. } = &mut self.content
            && notice.take().is_some()
        {
            return DetailsEvent::Consumed;
        }
        if let Content::Loaded { files, .. } = &mut self.content
            && let Some(review) = files.review_mut()
            && review.notice().is_some()
        {
            review.clear_notice();
            return DetailsEvent::Consumed;
        }
        let armed = matches!(self.content, Content::Loaded { prefix: true, .. });
        // The overlay's own command line eats every key while open.
        if let Content::Loaded { tab, command, .. } = &mut self.content
            && matches!(tab, DetailsTab::Files | DetailsTab::Conversation)
        {
            let review = *tab == DetailsTab::Files;
            if let Some(outcome) = command.handle_key(modifiers, code) {
                return match outcome {
                    CommandLineOutcome::Submit(input) if review => self.run_review_command(&input),
                    CommandLineOutcome::Submit(input) => self.run_comment_command(&input),
                    CommandLineOutcome::Cancel | CommandLineOutcome::Consumed => {
                        DetailsEvent::Consumed
                    }
                };
            }
        }
        // Esc and q reach a focused Files panel or an open commit diff first: they may
        // only leave the widget's visual mode, the comment panel or the commit table;
        // unconsumed they close below like everywhere else.
        let widget_focused = matches!(
            &self.content,
            Content::Loaded { tab: DetailsTab::Files, files, .. }
                if files.focus() != crate::view::dialog::files::Panel::Tree
        ) || matches!(
            &self.content,
            Content::Loaded { tab: DetailsTab::Commits, commits, .. }
                if commits.diff_open() && code == KeyCode::Esc
        ) || matches!(
            &self.content,
            Content::Loaded {
                tab: DetailsTab::Conversation,
                comment_focused: true,
                ..
            }
        );
        if !armed && !widget_focused && matches!(code, KeyCode::Esc | KeyCode::Char('q')) {
            return DetailsEvent::Close;
        }
        let Content::Loaded {
            content,
            scroll,
            focus,
            cursor,
            tab,
            prefix,
            commits,
            files,
            command,
            comment,
            comment_focused,
            posting,
            notice,
        } = &mut self.content
        else {
            return DetailsEvent::Consumed;
        };
        let changed: &[ChangedFile] = match &content.panes {
            Panes::Conversation => &[],
            Panes::Pull { files, .. } => files,
        };
        if std::mem::take(prefix) {
            if matches!(content.panes, Panes::Pull { .. }) {
                match code {
                    KeyCode::Char('j') => *tab = tab.next(),
                    KeyCode::Char('k') => *tab = tab.previous(),
                    KeyCode::Char(c) => {
                        if let Some(t) =
                            c.to_digit(10).and_then(|n| DetailsTab::ALL.get(n as usize))
                        {
                            *tab = *t;
                        }
                    }
                    _ => {}
                }
            }
            return DetailsEvent::Consumed;
        }
        if (modifiers, code) == (KeyModifiers::CONTROL, KeyCode::Char('t')) {
            *prefix = true;
            return DetailsEvent::Consumed;
        }
        match tab {
            DetailsTab::Conversation => {
                if *comment_focused && let Some(state) = comment {
                    if (modifiers, code) == (KeyModifiers::CONTROL, KeyCode::Char('d')) {
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
                        return DetailsEvent::Consumed;
                    }
                    match state.handle_events(modifiers, code) {
                        EventResult::Consumed => return DetailsEvent::Consumed,
                        EventResult::Unhandled(..) => {
                            if code == KeyCode::Esc {
                                state.set_focused(false);
                                if state.content().trim().is_empty() {
                                    *comment = None;
                                }
                                *comment_focused = false;
                            }
                            return DetailsEvent::Consumed;
                        }
                    }
                }
                match (modifiers, code) {
                    (KeyModifiers::NONE, KeyCode::Char('c')) => {
                        if *posting {
                            *notice = Some("busy".to_string());
                        } else {
                            let state = comment.get_or_insert_with(|| {
                                Box::new(crate::view::dialog::review::comment_field(""))
                            });
                            state.set_focused(true);
                            *comment_focused = true;
                        }
                        return DetailsEvent::Consumed;
                    }
                    (KeyModifiers::NONE, KeyCode::Char(':')) => {
                        command.open();
                        return DetailsEvent::Consumed;
                    }
                    (KeyModifiers::NONE, KeyCode::Tab | KeyCode::BackTab) if comment.is_some() => {
                        let boxes = content.boxes.len().max(1);
                        let forward = code == KeyCode::Tab;
                        if *comment_focused {
                            if let Some(state) = comment {
                                state.set_focused(false);
                                if state.content().trim().is_empty() {
                                    *comment = None;
                                }
                            }
                            *comment_focused = false;
                            *focus = if forward { 0 } else { boxes - 1 };
                        } else if (forward && *focus + 1 == boxes) || (!forward && *focus == 0) {
                            if let Some(state) = comment {
                                state.set_focused(true);
                            }
                            *comment_focused = true;
                        } else if forward {
                            *focus += 1;
                        } else {
                            *focus -= 1;
                        }
                        *cursor = 0;
                        return DetailsEvent::Consumed;
                    }
                    _ => {}
                }
            }
            DetailsTab::Commits => {
                commits.handle_key(modifiers, code);
                return match commits.take_request() {
                    Some(CommitRequest::Files { sha }) => match &content.panes {
                        Panes::Pull { owner, repo, .. } => DetailsEvent::FetchCommit(CommitRef {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            sha,
                        }),
                        Panes::Conversation => DetailsEvent::Consumed,
                    },
                    Some(CommitRequest::Blob { sha, path }) => {
                        match commit_blob_ref(&content.panes, sha, path) {
                            Some(blob) => DetailsEvent::Fetch(blob),
                            None => DetailsEvent::Consumed,
                        }
                    }
                    None => DetailsEvent::Consumed,
                };
            }
            DetailsTab::Files => {
                // `:` opens the overlay command line unless the comment editor is typing.
                if (modifiers, code) == (KeyModifiers::NONE, KeyCode::Char(':'))
                    && files.focus() != crate::view::dialog::files::Panel::Comment
                {
                    command.open();
                    return DetailsEvent::Consumed;
                }
                if !files.handle_key(changed, modifiers, code)
                    && matches!(code, KeyCode::Esc | KeyCode::Char('q'))
                {
                    return DetailsEvent::Close;
                }
                return match files
                    .take_request()
                    .and_then(|p| blob_ref(&content.panes, p))
                {
                    Some(blob) => DetailsEvent::Fetch(blob),
                    None => DetailsEvent::Consumed,
                };
            }
        }
        let boxes = content.boxes.len().max(1);
        let entries = content.boxes.get(*focus).map_or(0, |b| b.links.len());
        match code {
            KeyCode::Char('j') => *scroll += 1,
            KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::Tab => (*focus, *cursor) = ((*focus + 1) % boxes, 0),
            KeyCode::BackTab => (*focus, *cursor) = ((*focus + boxes - 1) % boxes, 0),
            KeyCode::Down => *cursor = (*cursor + 1).min(entries.saturating_sub(1)),
            KeyCode::Up => *cursor = cursor.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(link) = content.boxes[*focus].links.get(*cursor) {
                    return DetailsEvent::Open(link.clone());
                }
            }
            _ => {}
        }
        DetailsEvent::Consumed
    }

    /// Executes one submitted overlay command; unknown input becomes a status notice.
    fn run_review_command(&mut self, input: &str) -> DetailsEvent {
        let Content::Loaded { content, files, .. } = &mut self.content else {
            return DetailsEvent::Consumed;
        };
        let Panes::Pull {
            pull_id,
            head_oid,
            files: changed,
            ..
        } = &content.panes
        else {
            return DetailsEvent::Consumed;
        };
        let Some(review) = files.review_mut() else {
            return DetailsEvent::Consumed;
        };
        let words: Vec<&str> = input.split_whitespace().collect();
        match words.first().copied() {
            None => DetailsEvent::Consumed,
            Some("review") => {
                if review.submitting {
                    review.set_notice("review busy");
                } else if review.active {
                    review.set_notice("review already started");
                } else {
                    review.active = true;
                }
                DetailsEvent::Consumed
            }
            Some("submit") => {
                let verdict = match words.get(1).copied() {
                    Some("approve") => Some(Verdict::Approve),
                    Some("changes") => Some(Verdict::RequestChanges),
                    Some("comment") => Some(Verdict::Comment),
                    _ => None,
                };
                let Some(verdict) = verdict else {
                    review.set_notice("usage: submit approve|changes|comment [summary]");
                    return DetailsEvent::Consumed;
                };
                if !review.active {
                    review.set_notice("no review: run :review");
                    return DetailsEvent::Consumed;
                }
                if review.submitting {
                    review.set_notice("review busy");
                    return DetailsEvent::Consumed;
                }
                review.save_editor();
                let (threads, replies) = review.pending();
                review.submitting = true;
                let action = ReviewAction::Submit {
                    pull_id: pull_id.clone(),
                    head_oid: head_oid.clone(),
                    review_id: review.review_id.clone(),
                    threads,
                    replies,
                    verdict,
                    body: words[2..].join(" "),
                };
                files.sync_marks_for(changed);
                DetailsEvent::Review(action)
            }
            Some("discard") => {
                if !review.active {
                    review.set_notice("no review: run :review");
                    return DetailsEvent::Consumed;
                }
                if review.submitting {
                    review.set_notice("review busy");
                    return DetailsEvent::Consumed;
                }
                review.clear_local();
                review.active = false;
                let held = review.review_id.take();
                files.sync_marks_for(changed);
                match held {
                    Some(review_id) => DetailsEvent::Review(ReviewAction::Discard { review_id }),
                    None => DetailsEvent::Consumed,
                }
            }
            Some(_) => {
                review.set_notice(&format!("unknown command: {input}"));
                DetailsEvent::Consumed
            }
        }
    }

    /// Executes one submitted conversation command; unknown input becomes a popup.
    fn run_comment_command(&mut self, input: &str) -> DetailsEvent {
        let Content::Loaded {
            content,
            comment,
            comment_focused,
            posting,
            notice,
            ..
        } = &mut self.content
        else {
            return DetailsEvent::Consumed;
        };
        let words: Vec<&str> = input.split_whitespace().collect();
        match words.as_slice() {
            [] => DetailsEvent::Consumed,
            ["submit"] => {
                if *posting {
                    *notice = Some("busy".to_string());
                    return DetailsEvent::Consumed;
                }
                let body = comment
                    .as_ref()
                    .map(|state| state.content().trim().to_string())
                    .unwrap_or_default();
                if body.is_empty() {
                    *notice = Some("no comment to submit".to_string());
                    return DetailsEvent::Consumed;
                }
                *posting = true;
                DetailsEvent::Comment {
                    subject_id: content.subject_id.clone(),
                    body,
                }
            }
            ["submit", ..] => {
                *notice = Some("usage: submit".to_string());
                DetailsEvent::Consumed
            }
            ["discard"] => {
                if *posting {
                    *notice = Some("busy".to_string());
                } else if comment.is_none() {
                    *notice = Some("no comment to discard".to_string());
                } else {
                    *comment = None;
                    *comment_focused = false;
                }
                DetailsEvent::Consumed
            }
            _ => {
                *notice = Some(format!("unknown command: {input}"));
                DetailsEvent::Consumed
            }
        }
    }

    /// Whether a conversation comment post is in flight, for routing its outcome.
    pub fn posting(&self) -> bool {
        matches!(self.content, Content::Loaded { posting: true, .. })
    }

    /// The request that loads these details anew; nothing before they are loaded.
    pub fn refetch(&self) -> Option<CommentRefetch> {
        let Content::Loaded { content, .. } = &self.content else {
            return None;
        };
        Some(match &content.panes {
            Panes::Pull { owner, repo, .. } => CommentRefetch::Pull(PullRequestRef {
                owner: owner.clone(),
                repo: repo.clone(),
                number: self.number,
            }),
            Panes::Conversation => CommentRefetch::Issue(content.subject_id.clone()),
        })
    }

    /// Applies a posted comment's outcome; success answers what to request anew.
    pub fn handle_comment(
        &mut self,
        result: Result<String, crate::github::GithubError>,
    ) -> Option<CommentRefetch> {
        let Content::Loaded {
            comment,
            comment_focused,
            posting,
            notice,
            ..
        } = &mut self.content
        else {
            return None;
        };
        *posting = false;
        match result {
            Ok(_) => {
                *comment = None;
                *comment_focused = false;
                self.refetch()
            }
            Err(error) => {
                *notice = Some(format!("comment failed: {error}"));
                None
            }
        }
    }

    /// Applies a finished review request's outcome to the panel and the status line;
    /// a successful submit answers the repository coordinates whose pull request
    /// details the caller requests anew.
    pub fn handle_review(&mut self, result: ReviewResult) -> Option<PullRequestRef> {
        let Content::Loaded { content, files, .. } = &mut self.content else {
            return None;
        };
        let (changed, coordinates): (&[ChangedFile], _) = match &content.panes {
            Panes::Conversation => (&[], None),
            Panes::Pull {
                files, owner, repo, ..
            } => (
                files,
                Some(PullRequestRef {
                    owner: owner.clone(),
                    repo: repo.clone(),
                    number: self.number,
                }),
            ),
        };
        let review = files.review_mut()?;
        review.submitting = false;
        let mut refetch = None;
        match result.outcome {
            Ok(ReviewOutcome::Submitted) => {
                review.clear_local();
                review.active = false;
                review.review_id = None;
                refetch = coordinates;
            }
            // Discarding already ended review mode locally; a failure below only reports.
            Ok(ReviewOutcome::Discarded) => {}
            Err(error) => {
                if review.active {
                    review.review_id = result.review_id;
                    review.drop_sent(result.added, result.replied);
                }
                review.set_notice(&format!("review failed: {error}"));
            }
        }
        files.sync_marks_for(changed);
        refetch
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let boxed = area.inner(INSET);
        Clear.render(boxed, buf);
        buf.set_style(boxed, theme::base());
        let block = Block::bordered()
            .style(theme::on_bg(theme::TEMPLATE.hi))
            .title(format!(" #{} {} ", self.number, self.title))
            .title_alignment(HorizontalAlignment::Center);
        let inner = block.inner(boxed).inner(Margin::new(1, 0));
        block.render(boxed, buf);
        let number = self.number;
        match &mut self.content {
            Content::Loading => notice::render_loading(inner, buf, self.loading),
            Content::Failed(message) => notice::render_error(inner, buf, message),
            Content::Loaded {
                content,
                scroll,
                focus,
                cursor,
                tab,
                commits,
                files,
                command,
                comment,
                posting,
                notice,
                ..
            } => {
                // The bottom row becomes the review status line while review mode is
                // active or a comment posts: only the state, its label centered on the
                // purple fill.
                let status = if *posting {
                    Some(" SUBMITTING ")
                } else {
                    files.review().filter(|r| r.active).map(|r| {
                        if r.submitting {
                            " SUBMITTING "
                        } else {
                            " REVIEW "
                        }
                    })
                };
                let inner = match status {
                    Some(label) => {
                        let [rest, line] =
                            Layout::vertical([Constraint::Min(0), Constraint::Length(1)])
                                .areas(inner);
                        let purple = Style::default()
                            .fg(theme::TEMPLATE.text_hi)
                            .bg(theme::TEMPLATE.review);
                        buf.set_style(line, purple);
                        let x = line.x + line.width.saturating_sub(label.len() as u16) / 2;
                        buf.set_stringn(x, line.y, label, line.width as usize, purple.bold());
                        rest
                    }
                    None => inner,
                };
                let body = match &content.panes {
                    Panes::Conversation => inner,
                    Panes::Pull { .. } => {
                        let [line, body] = Layout::horizontal([
                            Constraint::Length(tabs::TAB_LINE_WIDTH),
                            Constraint::Min(0),
                        ])
                        .areas(inner);
                        tabs::render_vertical_tabs(
                            line,
                            buf,
                            DetailsTab::ALL
                                .iter()
                                .map(|t| t.label().to_string())
                                .collect(),
                            tab.index(),
                        );
                        body
                    }
                };
                match (&content.panes, *tab) {
                    (Panes::Pull { .. }, DetailsTab::Commits) => {
                        commits.render(body, buf);
                    }
                    (Panes::Pull { files: changed, .. }, DetailsTab::Files) => {
                        files.render(changed, body, buf);
                        if command.is_open() {
                            render_command_panel(command, REVIEW_HELP, body, buf);
                        }
                    }
                    _ => {
                        let [left, bar] =
                            Layout::horizontal([Constraint::Min(0), Constraint::Length(BAR_WIDTH)])
                                .areas(body);
                        let left = match comment {
                            Some(state) => {
                                let height = (body.height / 3).clamp(3, 12);
                                let [cards, editor] = Layout::vertical([
                                    Constraint::Min(0),
                                    Constraint::Length(height),
                                ])
                                .areas(left);
                                let widget = MarkdownInputFieldBuilder::default()
                                    .border(Border::Full(Margin::new(1, 0)))
                                    .title(Some(" comment ".into()))
                                    .style(theme::input_field_style())
                                    .build()
                                    .expect("MarkdownInputField fields all default");
                                StatefulWidget::render(&widget, editor, buf, state.as_mut());
                                cards
                            }
                            None => left,
                        };
                        render_cards(content, number, scroll, left, buf);
                        render_bar(&content.boxes, *focus, *cursor, bar, buf);
                        if command.is_open() {
                            render_command_panel(command, COMMENT_HELP, body, buf);
                        }
                    }
                }
                if let Some(message) = files
                    .review()
                    .and_then(|r| r.notice())
                    .or(notice.as_deref())
                {
                    render_message_popup(message, inner, buf);
                }
            }
        }
    }
}

/// ` [<index>] <title> ` per tab, the active one selected.
/// A bordered card's text: title line, then its lines.
struct CardText {
    title: String,
    margin: Margin,
    lines: Vec<Line<'static>>,
    /// Markdown drawn below the lines; empty for none.
    body: String,
    /// Border color.
    color: Color,
}

impl CardText {
    /// Rows the card takes with its borders and padding at `width`.
    fn height(&self, width: u16) -> u16 {
        let text_width = width.saturating_sub(2 + 2 * self.margin.horizontal);
        self.lines.len() as u16
            + markdown_rows(&self.body, text_width)
            + 2
            + 2 * self.margin.vertical
    }

    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .style(theme::on_bg(self.color))
            .title(self.title);
        let inner = block.inner(area).inner(self.margin);
        block.render(area, buf);
        let [top, body] = Layout::vertical([
            Constraint::Length(self.lines.len() as u16),
            Constraint::Min(0),
        ])
        .areas(inner);
        Paragraph::new(self.lines)
            .style(theme::base())
            .render(top, buf);
        if !self.body.trim().is_empty() {
            let mut state = markdown_state(&self.body, false);
            StatefulWidget::render(&markdown_widget(), body, buf, &mut state);
        }
    }
}

/// A read-only, unfocused markdown state holding `body` plus one trailing empty line, with the
/// active line at the top or on that trailing line.
pub(crate) fn markdown_state(body: &str, active_last: bool) -> MarkdownInputFieldState {
    let mut inner = CodeInputFieldStateBuilder::default()
        .vim(true)
        .focused(false)
        .build()
        .expect("CodeInputFieldState fields all default");
    inner.set_content(&format!("{body}\n"));
    let active = if active_last {
        inner.lines().len() - 1
    } else {
        0
    };
    inner.set_active_line(active);
    inner.set_cursor_col(0);
    MarkdownInputFieldStateBuilder::default()
        .inner(inner)
        .build()
        .expect("MarkdownInputFieldState fields all default")
}

pub(crate) fn markdown_widget() -> MarkdownInputField {
    MarkdownInputFieldBuilder::default()
        .style(theme::input_field_style())
        .build()
        .expect("MarkdownInputField fields all default")
}

/// The display rows `body` wraps to at `width`, measured by the widget itself: rendered one
/// row tall with the active line on the trailing empty line, its row scroll settles on the
/// number of rows before that line.
pub(crate) fn markdown_rows(body: &str, width: u16) -> u16 {
    if body.trim().is_empty() || width == 0 {
        return 0;
    }
    let mut state = markdown_state(body, true);
    let mut scratch = Buffer::empty(Rect::new(0, 0, width, 1));
    StatefulWidget::render(&markdown_widget(), scratch.area, &mut scratch, &mut state);
    state.row_scroll() as u16
}

/// The boxes stacked top to bottom, each as tall as its entries, cut at the bottom; the focused
/// box's cursor entry, when it opens something, on the highlight background.
/// A review message in a centered bordered popup; the caller dismisses it on the next
/// key.
fn render_message_popup(message: &str, area: Rect, buf: &mut Buffer) {
    let width = (message.chars().count() as u16 + 4).clamp(20, area.width.min(64));
    let text_width = width.saturating_sub(2).max(1);
    let rows = (message.chars().count() as u16).div_ceil(text_width).max(1);
    let height = (rows + 2).min(area.height);
    let panel = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height,
    };
    Clear.render(panel, buf);
    let block = Block::bordered().style(theme::on_bg(theme::TEMPLATE.warning));
    let inner = block.inner(panel);
    block.render(panel, buf);
    Paragraph::new(message)
        .style(theme::base())
        .wrap(ratatui::widgets::Wrap { trim: true })
        .centered()
        .render(inner, buf);
}

/// The overlay's own command line: a centered bordered panel over the `Files Changed`
/// tab body; the widget draws its help box above the panel.
/// The Files Changed tab's review commands.
const REVIEW_HELP: &[(&str, &str)] = &[
    ("review", "start review mode"),
    (
        "submit approve|changes|comment [summary]",
        "submit the review",
    ),
    ("discard", "drop drafts and the review"),
];

/// The conversation views' comment commands.
const COMMENT_HELP: &[(&str, &str)] = &[
    ("submit", "post the comment"),
    ("discard", "drop the comment draft"),
];

fn render_command_panel(
    command: &mut CommandLineState,
    help: &[(&str, &str)],
    area: Rect,
    buf: &mut Buffer,
) {
    let width = area.width.saturating_sub(8).clamp(20, 60).min(area.width);
    // Borders, the widget's help box (its three entries plus its own borders) and the
    // input row; the widget anchors the help directly above the input row, so a bottom
    // input row keeps the help inside the panel.
    let height = 8.min(area.height);
    let panel = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    Clear.render(panel, buf);
    let block = Block::bordered()
        .style(theme::on_bg(theme::TEMPLATE.hi))
        .title(" command ");
    let inner = block.inner(panel);
    block.render(panel, buf);
    let input = Rect {
        x: inner.x,
        y: inner.bottom().saturating_sub(1),
        width: inner.width,
        height: inner.height.min(1),
    };
    let widget = CommandLineBuilder::default()
        .style(theme::input_field_style())
        .highlight_style(theme::on_bg(theme::TEMPLATE.hi))
        .error_style(theme::on_bg(theme::TEMPLATE.error))
        .help(
            help.iter()
                .map(|(usage, text)| (usage.to_string(), text.to_string()))
                .collect(),
        )
        .build()
        .expect("CommandLineBuilder fields all default");
    StatefulWidget::render(&widget, input, buf, command);
}

fn render_bar(boxes: &[SidebarBox], focus: usize, cursor: usize, area: Rect, buf: &mut Buffer) {
    let mut y = area.y;
    for (i, item) in boxes.iter().enumerate() {
        let mut lines = item.lines.clone();
        if i == focus
            && cursor < item.links.len()
            && let Some(line) = lines.get_mut(cursor)
        {
            *line = theme::highlighted(std::mem::take(line));
        }
        if lines.is_empty() {
            lines.push(Line::styled(
                "None",
                theme::on_bg(theme::TEMPLATE.placeholder),
            ));
        }
        let card = CardText {
            title: format!(" {} ", item.title),
            margin: BOX_MARGIN,
            lines,
            body: String::new(),
            color: if i == focus {
                theme::TEMPLATE.hi
            } else {
                theme::TEMPLATE.border
            },
        };
        let height = card.height(area.width).min(area.bottom().saturating_sub(y));
        if height < 2 {
            break;
        }
        card.render(Rect::new(area.x, y, area.width, height), buf);
        y += height;
    }
}

/// Name shown in a timeline box's title.
fn event_name(event: &Event) -> &'static str {
    match event {
        Event::Comment { .. } => "Comment",
        Event::Assigned { .. } => "Assigned",
        Event::Unassigned { .. } => "Unassigned",
        Event::Labeled { .. } => "Labeled",
        Event::Unlabeled { .. } => "Unlabeled",
        Event::Milestoned { .. } => "Milestoned",
        Event::Demilestoned { .. } => "Demilestoned",
        Event::Closed { .. } => "Closed",
        Event::Reopened => "Reopened",
        Event::Renamed { .. } => "Renamed",
        Event::Merged => "Merged",
        Event::ReviewRequested { .. } => "Review requested",
        Event::Reviewed { .. } => "Reviewed",
        Event::Referenced { .. } => "Referenced",
        Event::CrossReferenced { .. } => "Cross-referenced",
    }
}

/// Border color of a timeline box from the template.
fn event_color(event: &Event) -> Color {
    let colors = theme::TEMPLATE.timeline;
    match event {
        Event::Comment { .. } => colors.comment,
        Event::Assigned { .. } | Event::Unassigned { .. } => colors.assigned,
        Event::Labeled { .. } | Event::Unlabeled { .. } => colors.labeled,
        Event::Milestoned { .. } | Event::Demilestoned { .. } => colors.milestoned,
        Event::Closed { .. } => colors.closed,
        Event::Reopened => colors.reopened,
        Event::Renamed { .. } => colors.renamed,
        Event::Merged => colors.merged,
        Event::ReviewRequested { .. } => colors.review_requested,
        Event::Reviewed { state, .. } => match state {
            ReviewState::Approved => colors.approved,
            ReviewState::ChangesRequested => colors.changes_requested,
            _ => colors.reviewed,
        },
        Event::Referenced { .. } | Event::CrossReferenced { .. } => colors.referenced,
    }
}

fn badge(label: &Label) -> Span<'static> {
    let bg = label_color(&label.color).unwrap_or(theme::TEMPLATE.hi_bg);
    Span::styled(
        format!(" {} ", label.name),
        Style::default().fg(badge_text_color(bg)).bg(bg),
    )
}

fn review_state(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Pending => "pending",
        ReviewState::Approved => "approved",
        ReviewState::ChangesRequested => "changes requested",
        ReviewState::Commented => "commented",
        ReviewState::Dismissed => "dismissed",
    }
}

/// One box per timeline item: a padded comment body, or one event line.
fn timeline_card(item: &TimelineItem) -> CardText {
    let actor = item.actor.as_deref().unwrap_or("ghost");
    let date: String = item.created_at.chars().take(10).collect();
    let title = format!(" {} · @{actor} · {date} ", event_name(&item.event));
    let dim = theme::on_bg(theme::TEMPLATE.placeholder);
    let mut markdown = String::new();
    let (margin, lines): (Margin, Vec<Line<'static>>) = match &item.event {
        Event::Comment { body } => {
            markdown = body.clone();
            (CARD_MARGIN, vec![])
        }
        Event::Assigned { login } | Event::Unassigned { login } => {
            (BOX_MARGIN, vec![Line::raw(format!("@{login}"))])
        }
        Event::Labeled { label } | Event::Unlabeled { label } => {
            (BOX_MARGIN, vec![Line::from(badge(label))])
        }
        Event::Milestoned { title } | Event::Demilestoned { title } => {
            (BOX_MARGIN, vec![Line::raw(title.clone())])
        }
        Event::Closed { reason } => (
            BOX_MARGIN,
            vec![Line::styled(
                reason
                    .as_deref()
                    .unwrap_or("closed")
                    .to_lowercase()
                    .replace('_', " "),
                dim,
            )],
        ),
        Event::Reopened => (BOX_MARGIN, vec![Line::styled("reopened", dim)]),
        Event::Renamed { from, to } => (
            BOX_MARGIN,
            vec![Line::from(vec![
                Span::styled("from ", dim),
                Span::raw(from.clone()),
                Span::styled(" to ", dim),
                Span::raw(to.clone()),
            ])],
        ),
        Event::Merged => (BOX_MARGIN, vec![Line::styled("merged", dim)]),
        Event::ReviewRequested { reviewer } => {
            (BOX_MARGIN, vec![Line::raw(format!("@{reviewer}"))])
        }
        Event::Reviewed { state, body } => {
            markdown = body.clone();
            (BOX_MARGIN, vec![Line::styled(review_state(*state), dim)])
        }
        Event::Referenced { id, headline } => (
            BOX_MARGIN,
            vec![Line::from(vec![
                Span::styled(id.clone(), dim),
                Span::raw(format!(" {headline}")),
            ])],
        ),
        Event::CrossReferenced {
            number,
            title,
            repository,
        } => (
            BOX_MARGIN,
            vec![Line::from(vec![
                Span::raw(format!("#{number} {title}")),
                Span::styled(format!("  {repository}"), dim),
            ])],
        ),
    };
    CardText {
        title,
        margin,
        lines,
        body: markdown,
        color: event_color(&item.event),
    }
}

/// The cards stacked in a buffer as tall as they need, scrolled into `area`.
fn render_cards(
    content: &DetailsContent,
    number: u64,
    scroll: &mut usize,
    area: Rect,
    buf: &mut Buffer,
) {
    let mut cards = vec![CardText {
        title: format!(" #{number} "),
        margin: CARD_MARGIN,
        lines: vec![],
        body: content.body.clone(),
        color: theme::TEMPLATE.border,
    }];
    if content.timeline.is_empty() {
        cards.push(CardText {
            title: " Timeline ".to_string(),
            margin: BOX_MARGIN,
            lines: vec![Line::styled(
                "No activity",
                theme::on_bg(theme::TEMPLATE.placeholder),
            )],
            body: String::new(),
            color: theme::TEMPLATE.border,
        });
    }
    cards.extend(content.timeline.iter().map(timeline_card));
    let total: u16 = cards.iter().map(|c| c.height(area.width)).sum();
    let mut canvas = Buffer::empty(Rect::new(0, 0, area.width, total));
    canvas.set_style(canvas.area, theme::base());
    let mut y = 0;
    for card in cards {
        let height = card.height(area.width);
        card.render(Rect::new(0, y, area.width, height), &mut canvas);
        y += height;
    }
    *scroll = (*scroll).min((total as usize).saturating_sub(area.height as usize));
    for row in 0..area.height.min(total) {
        for x in 0..area.width {
            buf[(area.x + x, area.y + row)] = canvas[(x, row + *scroll as u16)].clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::pull::CommitAuthor;
    use crate::testkit::{render_buffer, render_rows};
    use ratatui::style::Modifier;

    fn content(body: &str, timeline: Vec<TimelineItem>) -> DetailsContent {
        DetailsContent {
            panes: Panes::Conversation,
            subject_id: "N_1".into(),
            title: "Fix crash".into(),
            state: "open",
            author: Some("octo".into()),
            body: body.into(),
            timeline,
            boxes: vec![
                SidebarBox {
                    title: "Reviewers",
                    lines: vec![Line::raw("@rev pending")],
                    links: vec![],
                },
                SidebarBox {
                    title: "Assignees",
                    lines: vec![],
                    links: vec![],
                },
                SidebarBox {
                    title: "Labels",
                    lines: vec![Line::raw(" bug ")],
                    links: vec![],
                },
            ],
        }
    }

    fn item(actor: Option<&str>, event: Event) -> TimelineItem {
        TimelineItem {
            actor: actor.map(String::from),
            created_at: "2026-09-04T10:00:00Z".into(),
            event,
        }
    }

    fn comment(actor: Option<&str>, body: &str) -> TimelineItem {
        item(actor, Event::Comment { body: body.into() })
    }

    /// Columns of the bar for an 80 column screen: inset 1, border 1, margin 1 on each side.
    const BAR_LEFT_80: usize = 80 - 1 - 1 - 1 - 30;

    fn bar_of(rows: &[String], left: usize) -> Vec<String> {
        rows.iter()
            .map(|r| r.chars().skip(left).take(30).collect())
            .collect()
    }

    fn left_of(rows: &[String]) -> Vec<String> {
        rows.iter()
            .map(|r| r.chars().take(BAR_LEFT_80).collect())
            .collect()
    }

    fn blank(r: &str) -> bool {
        r.chars().all(|c| c == '│' || c == ' ')
    }

    #[test]
    /// TU-R-059, TU-R-065 — centered overlay with a loading box until a result arrives; a failure shows its message in a box.
    fn ut_loading_then_failure() {
        let mut d = DetailsDialog::new(5, "Fix crash".into(), "Loading thing..");
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows[1].contains("#5 Fix crash"), "{}", rows[1]);
        assert!(rows[1].starts_with(" ┌"), "centered: {}", rows[1]);
        assert!(
            rows[0].trim().is_empty() && rows[23].trim().is_empty(),
            "{rows:?}"
        );
        let at = rows
            .iter()
            .position(|r| r.contains("Loading thing.."))
            .expect("loading box");
        assert!(
            (10..=13).contains(&at),
            "box centered in the overlay: {rows:?}"
        );
        let chars: Vec<char> = rows[at].chars().collect();
        let text_at = chars
            .windows(15)
            .position(|w| w.iter().collect::<String>() == "Loading thing..")
            .expect("text");
        assert_eq!(
            chars[text_at - 2..text_at],
            ['│', ' '],
            "boxed: {}",
            rows[at]
        );
        assert!(
            rows[at - 1].contains('┌') && rows[at + 1].contains('└'),
            "{rows:?}"
        );
        assert!(
            rows[at - 1].find('┌').expect("corner") > 10,
            "small box: {}",
            rows[at - 1]
        );
        assert!(rows[22].contains('└'), "{}", rows[22]);
        d.set_result(Err::<DetailsContent, _>("github: HTTP 401"));
        let rows = render_rows(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        let at = rows
            .iter()
            .position(|r| r.contains("github: HTTP 401"))
            .expect("error box");
        assert!(
            rows[at - 1].contains('┌') && rows[at + 1].contains('└'),
            "{rows:?}"
        );
    }

    #[test]
    /// TU-R-066, TU-E-028 — description card with the body alone inside a 2 by 1 margin; one padded box per comment titled with type, actor and date; `j`/`k` scroll within bounds.
    fn ut_description_and_comment_boxes() {
        let mut d = DetailsDialog::new(5, "Fix crash".into(), "L");
        let timeline = vec![comment(Some("a"), "LGTM"), comment(None, "ghost says hi")];
        d.set_result(Ok::<_, String>(content(
            "Fixes the crash on start.",
            timeline,
        )));
        let rows = render_rows(80, 30, |f| d.render(f.area(), f.buffer_mut()));
        let left = left_of(&rows);
        let card = left
            .iter()
            .position(|r| r.contains("┌ #5 "))
            .expect("description card");
        assert!(
            blank(&left[card + 1]),
            "vertical margin: {}",
            left[card + 1]
        );
        assert!(
            left[card + 2].starts_with(" │ │  Fixes the crash on start."),
            "horizontal margin: {}",
            left[card + 2]
        );
        assert!(
            blank(&left[card + 3]),
            "vertical margin: {}",
            left[card + 3]
        );
        assert!(left[card + 4].contains('└'), "{}", left[card + 4]);
        let first = card + 5;
        assert!(
            left[first].contains("┌ Comment · @a · 2026-09-04 "),
            "{}",
            left[first]
        );
        assert!(
            blank(&left[first + 1]),
            "comment box keeps the vertical margin: {}",
            left[first + 1]
        );
        assert!(left[first + 2].contains("LGTM"), "{}", left[first + 2]);
        assert!(blank(&left[first + 3]), "{}", left[first + 3]);
        assert!(left[first + 4].contains('└'), "{}", left[first + 4]);
        assert!(
            left[first + 5].contains("┌ Comment · @ghost · 2026-09-04 "),
            "{}",
            left[first + 5]
        );
        assert!(
            left[first + 7].contains("ghost says hi"),
            "{}",
            left[first + 7]
        );

        let mut d = DetailsDialog::new(5, "Fix crash".into(), "L");
        let many: Vec<TimelineItem> = (1..=30)
            .map(|n| comment(Some("a"), &format!("comment {n}")))
            .collect();
        d.set_result(Ok::<_, String>(content("body", many)));
        let rows = render_rows(80, 20, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains("comment 1")), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("comment 30")), "{rows:?}");
        for _ in 0..500 {
            assert_eq!(
                d.handle_key(KeyModifiers::NONE, KeyCode::Char('j')),
                DetailsEvent::Consumed
            );
        }
        let rows = render_rows(80, 20, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains("comment 30")),
            "scrolled to the end: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("comment 29")),
            "no scrolling past the end: {rows:?}"
        );
        for _ in 0..500 {
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('k'));
        }
        let rows = render_rows(80, 20, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains("Fix crash")), "{rows:?}");
    }

    #[test]
    /// TU-R-071, TU-E-029 — the focused Development box keeps a highlighted cursor moved by Down/Up and reset by Tab; Enter opens the cursor's link, elsewhere it does nothing.
    fn ut_development_cursor_and_open() {
        let mut d = DetailsDialog::new(1, "t".into(), "Loading");
        let issue = Link::Issue {
            id: "I_7".into(),
            number: 7,
            title: "Crash".into(),
        };
        let pull = Link::Pull {
            owner: "o".into(),
            repo: "r".into(),
            number: 9,
            title: "Fix".into(),
        };
        let mut c = content("body", vec![]);
        c.boxes.push(SidebarBox {
            title: "Development",
            lines: vec![Line::raw("#7 Crash"), Line::raw("#9 Fix")],
            links: vec![issue.clone(), pull.clone()],
        });
        d.set_result(Ok::<_, String>(c));
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            DetailsEvent::Consumed
        );
        for _ in 0..3 {
            d.handle_key(KeyModifiers::NONE, KeyCode::Tab);
        }
        let buf = render_buffer(80, 24, |f| d.render(f.area(), f.buffer_mut()));
        let bg_of = |text: &str| {
            let area = buf.area;
            for y in 0..area.height {
                for x in 0..area.width {
                    let run: String = (x..area.width).map(|x| buf[(x, y)].symbol()).collect();
                    if run.starts_with(text) {
                        return buf[(x, y)].bg;
                    }
                }
            }
            panic!("{text} not drawn");
        };
        assert_eq!(bg_of("#7 Crash"), theme::TEMPLATE.hi_bg, "cursor entry");
        assert_ne!(bg_of("#9 Fix"), theme::TEMPLATE.hi_bg, "other entry");
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Down),
            DetailsEvent::Consumed
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Down),
            DetailsEvent::Consumed,
            "clamped"
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            DetailsEvent::Open(pull)
        );
        d.handle_key(KeyModifiers::NONE, KeyCode::Up);
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            DetailsEvent::Open(issue.clone())
        );
        d.handle_key(KeyModifiers::NONE, KeyCode::Down);
        d.handle_key(KeyModifiers::NONE, KeyCode::BackTab);
        d.handle_key(KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            DetailsEvent::Open(issue),
            "reset"
        );
        d.handle_key(KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            DetailsEvent::Consumed,
            "Reviewers"
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Tab),
            DetailsEvent::Consumed
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            DetailsEvent::Consumed,
            "empty box"
        );
    }

    #[test]
    /// TU-R-078, TU-E-046, TU-E-049 — Enter on a commit row yields the commit files fetch with the repository coordinates; the arrived list yields the first file's content request at the commit's own id, and a content response routes by its commit id (the head commit's to the Files tab, the commit's to its diff); Esc inside the commit diff returns to the table without closing the overlay; `q` closes it.
    fn ut_commit_diff_routing() {
        let mut d = DetailsDialog::new(5, "Fix".into(), "Loading");
        let mut c = content("body", vec![]);
        c.panes = Panes::Pull {
            commits: vec![Commit {
                sha: "abc1234".into(),
                headline: "Fix crash".into(),
                author: CommitAuthor::User("octo".into()),
                date: "2026-09-03T10:00:00Z".into(),
            }],
            files: vec![ChangedFile {
                path: "src/main.rs".into(),
                previous_path: None,
                status: crate::github::files::FileStatus::Modified,
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1,2 @@\n alpha\n+beta\n".into()),
            }],
            owner: "o".into(),
            repo: "r".into(),
            head_oid: "abc".into(),
            pull_id: "PR_1".into(),
            threads: vec![],
        };
        assert_eq!(
            d.set_result(Ok::<_, String>(c)),
            Some(BlobRef {
                owner: "o".into(),
                repo: "r".into(),
                oid: "abc".into(),
                path: "src/main.rs".into(),
            })
        );
        d.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('1'));
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Enter),
            DetailsEvent::FetchCommit(CommitRef {
                owner: "o".into(),
                repo: "r".into(),
                sha: "abc1234".into(),
            })
        );
        assert_eq!(
            d.handle_commit(
                "abc1234",
                Ok::<_, String>(vec![ChangedFile {
                    path: "lib.rs".into(),
                    previous_path: None,
                    status: crate::github::files::FileStatus::Modified,
                    additions: 1,
                    deletions: 1,
                    patch: Some("@@ -1 +1 @@\n-x\n+y\n".into()),
                }])
            ),
            Some(BlobRef {
                owner: "o".into(),
                repo: "r".into(),
                oid: "abc1234".into(),
                path: "lib.rs".into(),
            }),
            "the first file's content is requested at the commit"
        );
        assert_eq!(
            d.handle_blob(
                "abc1234",
                "lib.rs",
                Ok::<_, String>(Blob::Text("y\n".into()))
            ),
            None
        );
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter()
                .any(|r| r.contains(" Files ") && r.contains(" lib.rs ")),
            "commit diff shown: {rows:?}"
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Esc),
            DetailsEvent::Consumed,
            "Esc returns to the table"
        );
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter()
                .any(|r| r.contains("abc1234") && r.contains("Fix crash")),
            "table is back: {rows:?}"
        );
        d.handle_key(KeyModifiers::NONE, KeyCode::Enter);
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('q')),
            DetailsEvent::Close,
            "q closes the overlay from the commit diff"
        );
    }

    #[test]
    /// TU-R-072, TU-E-034 — a pull request overlay shows the vertical tab line at its left and Ctrl+T then j/k/digit switches its tab; an issue overlay shows none and Ctrl+T does nothing.
    fn ut_pull_tabs() {
        let ctrl_t =
            |d: &mut DetailsDialog| d.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        let mut d = DetailsDialog::new(5, "Fix".into(), "Loading");
        let mut c = content("body", vec![]);
        c.panes = Panes::Pull {
            commits: vec![Commit {
                sha: "abc1234".into(),
                headline: "Fix crash".into(),
                author: CommitAuthor::User("octo".into()),
                date: "2026-09-03T10:00:00Z".into(),
            }],
            files: vec![ChangedFile {
                path: "src/main.rs".into(),
                previous_path: None,
                status: crate::github::files::FileStatus::Modified,
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1,2 @@\n alpha\n+beta\n".into()),
            }],
            owner: "o".into(),
            repo: "r".into(),
            head_oid: "abc".into(),
            pull_id: "PR_1".into(),
            threads: vec![],
        };
        d.set_result(Ok::<_, String>(c));
        let buf = render_buffer(100, 40, |f| d.render(f.area(), f.buffer_mut()));
        let rows = crate::testkit::buffer_rows(&buf);
        let column = crate::testkit::buffer_column(&buf, 4);
        let conversation = column.find("CONVERSATION").expect("caption");
        let commits = column.find("COMMITS").expect("caption");
        let files = column.find("FILES").expect("caption");
        assert!(conversation < commits && commits < files, "{column:?}");
        assert!(
            !rows[4].contains("Conversation"),
            "no horizontal tab line: {}",
            rows[4]
        );
        assert!(rows.iter().any(|r| r.contains(" Reviewers ")), "{rows:?}");
        assert_eq!(ctrl_t(&mut d), DetailsEvent::Consumed);
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('1')),
            DetailsEvent::Consumed
        );
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter()
                .any(|r| r.contains("Commits") && r.contains("┌") && !r.contains("[1]")),
            "{rows:?}"
        );
        assert!(
            rows.iter()
                .any(|r| r.contains("abc1234") && r.contains("Fix crash")),
            "{rows:?}"
        );
        assert!(!rows.iter().any(|r| r.contains(" Reviewers ")), "{rows:?}");
        ctrl_t(&mut d);
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('j'));
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter()
                .any(|r| r.contains(" Files ") && r.contains(" src/main.rs ")),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.matches("alpha").count() == 2),
            "patch shown while the content loads: {rows:?}"
        );
        assert_eq!(
            d.handle_blob(
                "abc",
                "src/main.rs",
                Ok::<_, String>(Blob::Text("alpha\nbeta\n".into()))
            ),
            None,
            "nothing else to request"
        );
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.matches("alpha").count() == 2)
                && rows.iter().any(|r| r.matches("beta").count() == 1),
            "{rows:?}"
        );
        d.handle_key(KeyModifiers::NONE, KeyCode::Tab);
        assert!(
            matches!(&d.content, Content::Loaded { focus: 0, files, .. } if files.focus() == crate::view::dialog::files::Panel::Diff),
            "Tab moves the panel focus, not the bar's"
        );
        ctrl_t(&mut d);
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('j'));
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains(" Reviewers ")),
            "wrapped to Conversation: {rows:?}"
        );
        ctrl_t(&mut d);
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('k'));
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains(" Files ")),
            "wrapped back to Files: {rows:?}"
        );
        ctrl_t(&mut d);
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('7'));
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains(" Files ")),
            "digit beyond the last tab: {rows:?}"
        );
        ctrl_t(&mut d);
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('q')),
            DetailsEvent::Consumed,
            "the prefix eats q"
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('q')),
            DetailsEvent::Close
        );

        let mut d = DetailsDialog::new(5, "Fix".into(), "Loading");
        d.set_result(Ok::<_, String>(content("body", vec![])));
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(!rows.iter().any(|r| r.contains("Conversation")), "{rows:?}");
        assert_eq!(ctrl_t(&mut d), DetailsEvent::Consumed);
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('1')),
            DetailsEvent::Consumed
        );
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains(" Reviewers ")), "{rows:?}");
    }

    #[test]
    /// TU-R-073 — keys in the Commits tab move the table's selection.
    fn ut_commit_selection() {
        let mut d = DetailsDialog::new(5, "Fix".into(), "Loading");
        let mut c = content("body", vec![]);
        c.panes = Panes::Pull {
            commits: (1..=3)
                .map(|i| Commit {
                    sha: format!("sha{i}"),
                    headline: format!("c{i}"),
                    author: CommitAuthor::Git("o".into()),
                    date: "2026-09-03T10:00:00Z".into(),
                })
                .collect(),
            files: vec![],
            owner: "o".into(),
            repo: "r".into(),
            head_oid: "abc".into(),
            pull_id: "PR_1".into(),
            threads: vec![],
        };
        d.set_result(Ok::<_, String>(c));
        d.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('1'));
        for _ in 0..5 {
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('j'));
        }
        assert!(
            matches!(&d.content, Content::Loaded { commits, .. } if commits.selected() == Some(2)),
            "clamped"
        );
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('k'));
        assert!(
            matches!(&d.content, Content::Loaded { commits, .. } if commits.selected() == Some(1))
        );
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter()
                .any(|r| r.contains("Commit ID") && r.contains("Description")),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("sha2") && r.contains("c2")),
            "{rows:?}"
        );
    }

    #[test]
    /// TU-R-066, TU-R-070 — every event type renders as a compact box with its name, its line and the template color of its type.
    fn ut_event_boxes() {
        let bug = Label {
            name: "bug".into(),
            color: "d73a4a".into(),
        };
        let timeline = vec![
            item(Some("o"), Event::Assigned { login: "b".into() }),
            item(Some("o"), Event::Unassigned { login: "b".into() }),
            item(Some("o"), Event::Labeled { label: bug.clone() }),
            item(Some("o"), Event::Unlabeled { label: bug }),
            item(Some("o"), Event::Milestoned { title: "v1".into() }),
            item(Some("o"), Event::Demilestoned { title: "v1".into() }),
            item(
                Some("o"),
                Event::Closed {
                    reason: Some("NOT_PLANNED".into()),
                },
            ),
            item(Some("o"), Event::Closed { reason: None }),
            item(Some("o"), Event::Reopened),
            item(
                Some("o"),
                Event::Renamed {
                    from: "Old".into(),
                    to: "New".into(),
                },
            ),
            item(Some("o"), Event::Merged),
            item(
                Some("o"),
                Event::ReviewRequested {
                    reviewer: "core".into(),
                },
            ),
            item(
                Some("r"),
                Event::Reviewed {
                    state: ReviewState::Approved,
                    body: "ship it".into(),
                },
            ),
            item(
                Some("r"),
                Event::Reviewed {
                    state: ReviewState::ChangesRequested,
                    body: String::new(),
                },
            ),
            item(
                Some("r"),
                Event::Reviewed {
                    state: ReviewState::Commented,
                    body: String::new(),
                },
            ),
            item(
                Some("o"),
                Event::Referenced {
                    id: "abc1234".into(),
                    headline: "Fix it".into(),
                },
            ),
            item(
                Some("o"),
                Event::CrossReferenced {
                    number: 9,
                    title: "Follow-up".into(),
                    repository: "o/r".into(),
                },
            ),
        ];
        let expected: Vec<(&str, &str, Color)> = {
            let c = theme::TEMPLATE.timeline;
            vec![
                ("Assigned", "@b", c.assigned),
                ("Unassigned", "@b", c.assigned),
                ("Labeled", " bug ", c.labeled),
                ("Unlabeled", " bug ", c.labeled),
                ("Milestoned", "v1", c.milestoned),
                ("Demilestoned", "v1", c.milestoned),
                ("Closed", "not planned", c.closed),
                ("Closed", "closed", c.closed),
                ("Reopened", "reopened", c.reopened),
                ("Renamed", "from Old to New", c.renamed),
                ("Merged", "merged", c.merged),
                ("Review requested", "@core", c.review_requested),
                ("Reviewed", "approved", c.approved),
                ("Reviewed", "changes requested", c.changes_requested),
                ("Reviewed", "commented", c.reviewed),
                ("Referenced", "abc1234 Fix it", c.referenced),
                ("Cross-referenced", "#9 Follow-up  o/r", c.referenced),
            ]
        };
        let mut d = DetailsDialog::new(5, "T".into(), "L");
        d.set_result(Ok::<_, String>(content("b", timeline)));
        let area = Rect::new(0, 0, 100, 80);
        let mut buf = Buffer::empty(area);
        d.render(area, &mut buf);
        let row =
            |y: u16| -> String { (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect() };
        let rows: Vec<String> = (0..80).map(row).collect();
        let description_end = rows
            .iter()
            .position(|r| r.contains("└") && r.contains("│ └"))
            .expect("description end");
        let mut y = description_end + 1;
        for (name, line, color) in expected {
            let title = format!("┌ {name} · @");
            assert!(
                rows[y].contains(&title),
                "box {name} at row {y}: {}",
                rows[y]
            );
            assert!(rows[y + 1].contains(line), "{name} line: {}", rows[y + 1]);
            let height = if line == "approved" { 4 } else { 3 };
            assert!(
                rows[y + height - 1].contains('└'),
                "{name} compact: {}",
                rows[y + height - 1]
            );
            let x = rows[y].find('┌').expect("corner");
            let x = rows[y][..x].chars().count() as u16;
            assert_eq!(buf[(x, y as u16)].fg, color, "{name} border color");
            y += height;
        }
        let ship = rows
            .iter()
            .position(|r| r.contains("ship it"))
            .expect("review body");
        assert!(rows[ship - 1].contains("approved"), "{}", rows[ship - 1]);
    }

    #[test]
    /// TU-R-066, TU-E-027 — a body line wider than the card wraps; without timeline items one box reads `No activity`.
    fn ut_wrapping_and_empty_timeline() {
        let mut d = DetailsDialog::new(5, "T".into(), "L");
        let mut c = content(&"word ".repeat(40), vec![]);
        c.author = None;
        d.set_result(Ok::<_, String>(c));
        let rows = render_rows(90, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().filter(|r| r.contains("word word")).count() >= 3,
            "{rows:?}"
        );
        assert!(rows.iter().all(|r| r.chars().count() <= 90), "{rows:?}");
        let card = rows
            .iter()
            .position(|r| r.contains("┌ Timeline "))
            .expect("card");
        assert!(rows[card + 1].contains("No activity"), "{}", rows[card + 1]);
        assert!(rows[card + 2].contains('└'), "{}", rows[card + 2]);
    }

    #[test]
    /// TU-R-077, TU-E-043 — description, comment and review bodies render as markdown: heading markers, emphasis markers and backticks hidden, bullets as `•`, quotes with a bar, no source line revealed and no row highlighted; an empty comment body takes no rows.
    fn ut_markdown_bodies() {
        let mut d = DetailsDialog::new(5, "T".into(), "L");
        let timeline = vec![
            item(
                Some("a"),
                Event::Comment {
                    body: "> quoted\n- [x] done".into(),
                },
            ),
            item(Some("b"), Event::Comment { body: "".into() }),
            item(
                Some("c"),
                Event::Reviewed {
                    state: ReviewState::Approved,
                    body: "## Fine\n[link](https://x.y)".into(),
                },
            ),
        ];
        let c = content("# Head\n- item\n`code` and **bold**", timeline);
        d.set_result(Ok::<_, String>(c));
        let buf = render_buffer(90, 40, |f| d.render(f.area(), f.buffer_mut()));
        let rows = crate::testkit::buffer_rows(&buf);
        let find = |text: &str| {
            rows.iter()
                .position(|r| r.contains(text))
                .unwrap_or_else(|| panic!("{text} missing in {rows:?}"))
        };
        let head = find("Head");
        assert!(!rows[head].contains("# Head"), "{}", rows[head]);
        let x = rows[head][..rows[head].find("Head").expect("x")]
            .chars()
            .count() as u16;
        assert!(
            buf[(x, head as u16)].modifier.contains(Modifier::BOLD),
            "heading bold"
        );
        assert_eq!(buf[(x, head as u16)].bg, theme::BG, "no row highlight");
        assert!(rows[head + 1].contains("• item"), "{}", rows[head + 1]);
        assert!(
            rows[head + 2].contains("code and bold") && !rows[head + 2].contains('`'),
            "{}",
            rows[head + 2]
        );
        let quoted = find("quoted");
        assert!(
            rows[quoted].contains("▎") && !rows[quoted].contains('>'),
            "{}",
            rows[quoted]
        );
        assert!(rows[quoted + 1].contains("☑ done"), "{}", rows[quoted + 1]);
        let empty = find("Comment · @b");
        assert!(
            rows[empty + 3].contains('└'),
            "empty body: {:?}",
            &rows[empty..empty + 4]
        );
        let fine = find("Fine");
        assert!(
            rows[fine - 1].contains("approved") && !rows[fine].contains("##"),
            "{:?}",
            &rows[fine - 1..=fine]
        );
        assert!(
            rows[fine + 1].contains("link") && !rows[fine + 1].contains("https"),
            "{}",
            rows[fine + 1]
        );
    }

    #[test]
    /// TU-R-068, TU-R-069, TU-E-026 — the bar stacks the boxes with `None` for empty ones, clipped at the bottom; the first box is focused and Tab cycles the focus, the left content still scrolls.
    fn ut_bar_boxes_and_focus() {
        let mut d = DetailsDialog::new(5, "T".into(), "L");
        d.set_result(Ok::<_, String>(content("body", vec![])));
        let rows = render_rows(80, 30, |f| d.render(f.area(), f.buffer_mut()));
        let bar = bar_of(&rows, BAR_LEFT_80);
        let at = |title: &str| {
            bar.iter()
                .position(|r| r.contains(title))
                .unwrap_or_else(|| panic!("{title}: {bar:?}"))
        };
        assert!(
            at("Reviewers") < at("Assignees") && at("Assignees") < at("Labels"),
            "{bar:?}"
        );
        assert!(
            bar[at("Reviewers") + 1].contains("@rev pending"),
            "{}",
            bar[at("Reviewers") + 1]
        );
        assert!(
            bar[at("Assignees") + 1].contains("None"),
            "{}",
            bar[at("Assignees") + 1]
        );
        assert!(
            bar[at("Labels") + 1].contains("bug"),
            "{}",
            bar[at("Labels") + 1]
        );
        assert!(
            bar[at("Reviewers")].starts_with('┌'),
            "bar starts at its own border: {}",
            bar[at("Reviewers")]
        );

        let focused = |d: &mut DetailsDialog| -> Vec<&'static str> {
            let area = Rect::new(0, 0, 80, 30);
            let mut buf = Buffer::empty(area);
            d.render(area, &mut buf);
            let mut out = Vec::new();
            for title in ["Reviewers", "Assignees", "Labels"] {
                let row = (0..30u16)
                    .find(|y| {
                        (0..80u16).any(|x| {
                            let s: String = (x..(x + title.len() as u16).min(80))
                                .map(|xx| buf[(xx, *y)].symbol().to_string())
                                .collect();
                            s == title
                        })
                    })
                    .expect("box title row");
                let corner = (BAR_LEFT_80 as u16..80)
                    .find(|x| buf[(*x, row)].symbol() == "┌")
                    .expect("corner");
                if buf[(corner, row)].fg == theme::TEMPLATE.hi {
                    out.push(title);
                }
            }
            out
        };
        assert_eq!(focused(&mut d), vec!["Reviewers"]);
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Tab),
            DetailsEvent::Consumed
        );
        assert_eq!(focused(&mut d), vec!["Assignees"]);
        d.handle_key(KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(focused(&mut d), vec!["Labels"]);
        d.handle_key(KeyModifiers::NONE, KeyCode::Tab);
        assert_eq!(focused(&mut d), vec!["Reviewers"], "wraps to the first");
        d.handle_key(KeyModifiers::NONE, KeyCode::BackTab);
        assert_eq!(focused(&mut d), vec!["Labels"], "reverse wraps to the last");
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('j'));
        assert_eq!(focused(&mut d), vec!["Labels"], "scrolling keeps the focus");

        let rows = render_rows(80, 8, |f| d.render(f.area(), f.buffer_mut()));
        let bar = bar_of(&rows, BAR_LEFT_80);
        assert!(bar.iter().any(|r| r.contains("Reviewers")), "{bar:?}");
        assert!(
            !bar.iter().any(|r| r.contains("Labels")),
            "clipped: {bar:?}"
        );
    }

    #[test]
    /// TU-R-061, TU-R-067 — Esc and `q` close; other keys are consumed.
    fn ut_close_keys() {
        let mut d = DetailsDialog::new(5, "T".into(), "L");
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Esc),
            DetailsEvent::Close
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('q')),
            DetailsEvent::Close
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('x')),
            DetailsEvent::Consumed
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Tab),
            DetailsEvent::Consumed
        );
    }

    fn pull_dialog() -> DetailsDialog {
        let mut d = DetailsDialog::new(5, "Fix".into(), "Loading");
        let mut c = content("body", vec![]);
        c.panes = Panes::Pull {
            commits: vec![],
            files: vec![ChangedFile {
                path: "a.rs".into(),
                previous_path: None,
                status: crate::github::files::FileStatus::Modified,
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1,3 +1,3 @@\n fn main() {\n-    old();\n+    new();\n }\n".into()),
            }],
            owner: "o".into(),
            repo: "r".into(),
            head_oid: "abc".into(),
            pull_id: "PR_1".into(),
            threads: vec![],
        };
        d.set_result(Ok::<_, String>(c));
        d
    }

    fn command(d: &mut DetailsDialog, text: &str) -> DetailsEvent {
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char(':')),
            DetailsEvent::Consumed
        );
        for c in text.chars() {
            d.handle_key(KeyModifiers::NONE, KeyCode::Char(c));
        }
        d.handle_key(KeyModifiers::NONE, KeyCode::Enter)
    }

    /// The overlay's bottom inner row, where the review status line renders.
    const STATUS_Y: u16 = 30 - INSET.vertical - 2;

    fn status_row(d: &mut DetailsDialog) -> String {
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        rows[STATUS_Y as usize].clone()
    }

    /// Whether any rendered row holds `text`, for popup assertions.
    fn shows(d: &mut DetailsDialog, text: &str) -> bool {
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        rows.iter().any(|r| r.contains(text))
    }

    /// Asserts the message popup holds `text`, then dismisses it with one consumed key.
    fn dismiss(d: &mut DetailsDialog, text: &str) {
        assert!(shows(d, text), "popup missing: {text}");
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('x')),
            DetailsEvent::Consumed
        );
        assert!(!shows(d, text), "popup not dismissed: {text}");
    }

    #[test]
    /// TU-R-079, TU-E-051, TU-E-056, TU-E-058 — `:` on the `Files Changed` tab opens the overlay's own centered bordered command line with the review help, Enter runs the trimmed input and Esc only closes; the conversation view opens it with the comment help and the `Commits` tab not at all; unknown input, a bad `submit` verdict and a second `review` each land in the message popup.
    fn ut_review_command_line() {
        let mut d = pull_dialog();
        d.handle_key(KeyModifiers::NONE, KeyCode::Char(':'));
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.contains(" command "))
                && rows.iter().any(|r| r.contains("post the comment"))
                && !rows.iter().any(|r| r.contains("start review mode")),
            "the conversation opens the comment command line: {rows:?}"
        );
        d.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        d.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('1'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Char(':'));
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            !rows.iter().any(|r| r.contains(" command ")),
            "no command line on the Commits tab: {rows:?}"
        );
        d.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('2'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Char(':'));
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(rows.iter().any(|r| r.contains(" command ")), "{rows:?}");
        assert!(
            rows.iter().any(|r| r.contains("review"))
                && rows
                    .iter()
                    .any(|r| r.contains("submit approve|changes|comment [summary]")),
            "help shown: {rows:?}"
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Esc),
            DetailsEvent::Consumed,
            "Esc only closes the panel"
        );
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(!rows.iter().any(|r| r.contains(" command ")), "{rows:?}");

        assert_eq!(command(&mut d, "zzz sub"), DetailsEvent::Consumed);
        assert!(
            !status_row(&mut d).contains("unknown command"),
            "messages never land on the status line: {}",
            status_row(&mut d)
        );
        dismiss(&mut d, "unknown command: zzz sub");
        assert_eq!(command(&mut d, "submit"), DetailsEvent::Consumed);
        dismiss(&mut d, "usage: submit approve|changes|comment [summary]");
        assert_eq!(command(&mut d, "review"), DetailsEvent::Consumed);
        assert_eq!(command(&mut d, "review"), DetailsEvent::Consumed);
        dismiss(&mut d, "review already started");
    }

    #[test]
    /// TU-R-080, TU-R-084, TU-E-052, TU-E-054, TU-E-055, TU-E-057 — `review` activates review mode and the bottom row shows ` REVIEW ` on the purple background; `submit` with no drafts sends the verdict and summary alone and locks the review while it runs; a failed submit keeps review mode with the created review id, a successful one ends it and yields the pull request reference to refetch; `discard` outside review mode notices, inside it drops the held review; `q` still closes the overlay.
    fn ut_review_submit_and_discard() {
        let mut d = pull_dialog();
        d.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('2'));
        assert_eq!(command(&mut d, "discard"), DetailsEvent::Consumed);
        dismiss(&mut d, "no review: run :review");
        assert_eq!(command(&mut d, "review"), DetailsEvent::Consumed);
        let buf = render_buffer(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        let rows = crate::testkit::buffer_rows(&buf);
        let row = &rows[STATUS_Y as usize];
        let x = row.find("REVIEW").expect("label") as u16;
        assert_eq!(
            buf[(x, STATUS_Y)].bg,
            theme::TEMPLATE.review,
            "purple label"
        );
        let edges = (INSET.horizontal + 2, 100 - INSET.horizontal - 3);
        assert_eq!(
            (buf[(edges.0, STATUS_Y)].bg, buf[(edges.1, STATUS_Y)].bg),
            (theme::TEMPLATE.review, theme::TEMPLATE.review),
            "purple fills the full row"
        );
        assert_eq!(
            row.trim_matches(['│', ' ']),
            "REVIEW",
            "only the state on the line: {row}"
        );
        let center = edges.0 + (edges.1 - edges.0) / 2;
        assert!(
            (x..x + 6).contains(&center),
            "label centered: x {x}, center {center}"
        );

        assert_eq!(
            command(&mut d, "submit comment  nice work"),
            DetailsEvent::Review(ReviewAction::Submit {
                pull_id: "PR_1".into(),
                head_oid: "abc".into(),
                review_id: None,
                threads: vec![],
                replies: vec![],
                verdict: Verdict::Comment,
                body: "nice work".into(),
            })
        );
        assert_eq!(
            status_row(&mut d).trim_matches(['│', ' ']),
            "SUBMITTING",
            "the label is the state while the request runs"
        );
        assert_eq!(command(&mut d, "discard"), DetailsEvent::Consumed);
        dismiss(&mut d, "review busy");
        assert_eq!(
            d.handle_review(ReviewResult {
                review_id: Some("R_1".into()),
                added: 0,
                replied: 0,
                outcome: Err(crate::github::GithubError::Status(502)),
            }),
            None,
            "a failure refetches nothing"
        );
        dismiss(&mut d, "review failed: github: HTTP 502");
        assert_eq!(
            status_row(&mut d).trim_matches(['│', ' ']),
            "REVIEW",
            "a failed submit keeps review mode"
        );
        assert_eq!(
            command(&mut d, "discard"),
            DetailsEvent::Review(ReviewAction::Discard {
                review_id: "R_1".into(),
            })
        );
        assert!(
            !status_row(&mut d).contains("REVIEW"),
            "discard ends review mode"
        );
        assert_eq!(
            d.handle_review(ReviewResult {
                review_id: None,
                added: 0,
                replied: 0,
                outcome: Ok(ReviewOutcome::Discarded),
            }),
            None,
            "a discard refetches nothing"
        );

        assert_eq!(command(&mut d, "review"), DetailsEvent::Consumed);
        assert!(matches!(
            command(&mut d, "submit approve"),
            DetailsEvent::Review(ReviewAction::Submit {
                verdict: Verdict::Approve,
                ..
            })
        ));
        assert_eq!(
            d.handle_review(ReviewResult {
                review_id: None,
                added: 0,
                replied: 0,
                outcome: Ok(ReviewOutcome::Submitted),
            }),
            Some(PullRequestRef {
                owner: "o".into(),
                repo: "r".into(),
                number: 5,
            }),
            "a successful submit requests the details anew"
        );
        assert!(
            !status_row(&mut d).contains("REVIEW"),
            "a successful submit ends review mode"
        );
        assert_eq!(
            d.handle_key(KeyModifiers::NONE, KeyCode::Char('q')),
            DetailsEvent::Close
        );
    }

    #[test]
    /// TU-R-085 — Tab while the comment editor is in Insert mode indents four spaces instead of cycling the focus.
    fn ut_conversation_tab_in_insert_mode_indents() {
        let mut d = pull_dialog();
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('c'));
        for key in [KeyCode::Char('i'), KeyCode::Char('x'), KeyCode::Enter] {
            d.handle_key(KeyModifiers::NONE, key);
        }
        d.handle_key(KeyModifiers::NONE, KeyCode::Tab);
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('y'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        d.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        assert_eq!(
            command(&mut d, "submit"),
            DetailsEvent::Comment {
                subject_id: "N_1".into(),
                body: "x\n    y".into(),
            }
        );
    }

    #[test]
    /// TU-R-085, TU-R-086, TU-E-059, TU-E-060, TU-E-062 — `c` on the conversation opens the comment editor box which keeps its draft when the focus leaves and closes when blank; bare `submit` posts the draft with the details' node id and shows SUBMITTING; arguments, a missing draft or a running post answer their popups; `discard` drops the draft; a failure keeps it, success requests the details anew.
    fn ut_conversation_comment() {
        let mut d = pull_dialog();
        assert_eq!(command(&mut d, "submit"), DetailsEvent::Consumed);
        dismiss(&mut d, "no comment to submit");
        assert_eq!(command(&mut d, "discard"), DetailsEvent::Consumed);
        dismiss(&mut d, "no comment to discard");

        d.handle_key(KeyModifiers::NONE, KeyCode::Char('c'));
        assert!(shows(&mut d, " comment "), "editor box under the cards");
        for key in [
            KeyCode::Char('i'),
            KeyCode::Char('h'),
            KeyCode::Char('i'),
            KeyCode::Esc,
        ] {
            d.handle_key(KeyModifiers::NONE, key);
        }
        d.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        assert!(
            shows(&mut d, " comment ") && shows(&mut d, "hi"),
            "the box keeps its draft when the focus leaves"
        );
        assert_eq!(command(&mut d, "submit now"), DetailsEvent::Consumed);
        dismiss(&mut d, "usage: submit");
        assert_eq!(command(&mut d, "discard"), DetailsEvent::Consumed);
        assert!(!shows(&mut d, " comment "), "discard drops the draft");

        d.handle_key(KeyModifiers::NONE, KeyCode::Char('c'));
        d.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        assert!(!shows(&mut d, " comment "), "a blank draft closes the box");

        d.handle_key(KeyModifiers::NONE, KeyCode::Char('c'));
        for key in [
            KeyCode::Char('i'),
            KeyCode::Char('o'),
            KeyCode::Char('k'),
            KeyCode::Esc,
        ] {
            d.handle_key(KeyModifiers::NONE, key);
        }
        d.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        assert_eq!(
            command(&mut d, "submit"),
            DetailsEvent::Comment {
                subject_id: "N_1".into(),
                body: "ok".into(),
            }
        );
        assert_eq!(status_row(&mut d).trim_matches(['│', ' ']), "SUBMITTING");
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('c'));
        dismiss(&mut d, "busy");
        d.handle_comment(Err(crate::github::GithubError::Status(502)));
        dismiss(&mut d, "comment failed: github: HTTP 502");
        assert!(shows(&mut d, " comment "), "a failure keeps the draft");
        assert!(matches!(
            command(&mut d, "submit"),
            DetailsEvent::Comment { .. }
        ));
        assert_eq!(
            d.handle_comment(Ok("IC_1".into())),
            Some(CommentRefetch::Pull(PullRequestRef {
                owner: "o".into(),
                repo: "r".into(),
                number: 5,
            })),
            "success requests the pull request anew"
        );
        assert!(!shows(&mut d, " comment "), "success drops the draft");

        let mut d = DetailsDialog::new(7, "Crash".into(), "Loading");
        d.set_result(Ok::<_, String>(content("body", vec![])));
        d.handle_key(KeyModifiers::NONE, KeyCode::Char('c'));
        for key in [KeyCode::Char('i'), KeyCode::Char('y'), KeyCode::Esc] {
            d.handle_key(KeyModifiers::NONE, key);
        }
        d.handle_key(KeyModifiers::NONE, KeyCode::Esc);
        assert!(matches!(
            command(&mut d, "submit"),
            DetailsEvent::Comment { .. }
        ));
        assert_eq!(
            d.handle_comment(Ok("IC_2".into())),
            Some(CommentRefetch::Issue("N_1".into())),
            "an issue overlay refetches by node id"
        );
    }
}
