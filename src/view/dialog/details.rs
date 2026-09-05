//! Details overlay shared by issues and pull requests: a description card and one box per
//! timeline item at the left, scrolled together, and a bar of focusable boxes at the right.

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::state::{
    CodeInputFieldStateBuilder, MarkdownInputFieldState, MarkdownInputFieldStateBuilder,
};
use ferrowl_ui::widgets::{MarkdownInputField, MarkdownInputFieldBuilder};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, HorizontalAlignment, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, StatefulWidget, Widget};

use crate::github::blob::Blob;
use crate::github::board::Label;
use crate::github::files::ChangedFile;
use crate::github::pull::{Commit, ReviewState};
use crate::github::timeline::{Event, TimelineItem};
use crate::view::board::{badge_text_color, label_color};
use crate::view::dialog::{commits::CommitsView, files::FilesState};
use crate::view::{notice, tabs, theme};

/// Screen cells left free around the overlay on each side.
const INSET: Margin = Margin::new(4, 3);

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
                files: Box::new(FilesState::new(match &content.panes {
                    Panes::Conversation => &[],
                    Panes::Pull { files, .. } => files,
                })),
                content: Box::new(content),
                scroll: 0,
                focus: 0,
                cursor: 0,
                tab: DetailsTab::Conversation,
                prefix: false,
            },
            Err(e) => Content::Failed(e.to_string()),
        };
        self.take_request()
    }

    /// Stores a file's content and returns the next file whose content is needed, if any.
    pub fn handle_blob(
        &mut self,
        path: &str,
        result: Result<Blob, impl ToString>,
    ) -> Option<BlobRef> {
        if let Content::Loaded { content, files, .. } = &mut self.content
            && let Panes::Pull { files: changed, .. } = &content.panes
        {
            files.handle_blob(changed, path, result);
        }
        self.take_request()
    }

    fn take_request(&mut self) -> Option<BlobRef> {
        let Content::Loaded { content, files, .. } = &mut self.content else {
            return None;
        };
        let path = files.take_request()?;
        blob_ref(&content.panes, path)
    }

    pub fn handle_key(&mut self, modifiers: KeyModifiers, code: KeyCode) -> DetailsEvent {
        let armed = matches!(self.content, Content::Loaded { prefix: true, .. });
        if !armed && matches!(code, KeyCode::Esc | KeyCode::Char('q')) {
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
            DetailsTab::Conversation => {}
            DetailsTab::Commits => {
                commits.handle_key(modifiers, code);
                return DetailsEvent::Consumed;
            }
            DetailsTab::Files => {
                files.handle_key(changed, modifiers, code);
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
                ..
            } => {
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
                    }
                    _ => {
                        let [left, bar] =
                            Layout::horizontal([Constraint::Min(0), Constraint::Length(BAR_WIDTH)])
                                .areas(body);
                        render_cards(content, number, scroll, left, buf);
                        render_bar(&content.boxes, *focus, *cursor, bar, buf);
                    }
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
fn markdown_state(body: &str, active_last: bool) -> MarkdownInputFieldState {
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

fn markdown_widget() -> MarkdownInputField {
    MarkdownInputFieldBuilder::default()
        .style(theme::input_field_style())
        .build()
        .expect("MarkdownInputField fields all default")
}

/// The display rows `body` wraps to at `width`, measured by the widget itself: rendered one
/// row tall with the active line on the trailing empty line, its row scroll settles on the
/// number of rows before that line.
fn markdown_rows(body: &str, width: u16) -> u16 {
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
    let author = content.author.as_deref().unwrap_or("ghost");
    let description: Vec<Line<'static>> = vec![
        Line::styled(content.title.clone(), theme::on_bg(theme::TEMPLATE.text_hi)),
        Line::from(vec![
            Span::styled(content.state, theme::on_bg(theme::TEMPLATE.hi)),
            Span::raw("  by "),
            Span::styled(format!("@{author}"), theme::on_bg(theme::TEMPLATE.text_hi)),
        ]),
        Line::raw(""),
    ];
    let mut cards = vec![CardText {
        title: format!(" #{number} "),
        margin: CARD_MARGIN,
        lines: description,
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

    /// Columns of the bar for an 80 column screen: inset 4, border 1, margin 1 on each side.
    const BAR_LEFT_80: usize = 80 - 4 - 1 - 1 - 30;

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
        assert!(rows[3].contains("#5 Fix crash"), "{}", rows[3]);
        assert!(rows[3].starts_with("    ┌"), "centered: {}", rows[3]);
        assert!(
            rows[2].trim().is_empty() && rows[21].trim().is_empty(),
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
        assert!(rows[20].contains('└'), "{}", rows[20]);
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
    /// TU-R-066, TU-E-028 — description card with title, state and author line and body inside a 2 by 1 margin; one padded box per comment titled with type, actor and date; `j`/`k` scroll within bounds.
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
            left[card + 2].starts_with("    │ │  Fix crash"),
            "horizontal margin: {}",
            left[card + 2]
        );
        assert!(
            left[card + 3].contains("open") && left[card + 3].contains("by @octo"),
            "{}",
            left[card + 3]
        );
        assert!(blank(&left[card + 4]), "empty line: {}", left[card + 4]);
        assert!(
            left[card + 5].contains("Fixes the crash on start."),
            "{}",
            left[card + 5]
        );
        assert!(
            blank(&left[card + 6]),
            "vertical margin: {}",
            left[card + 6]
        );
        assert!(left[card + 7].contains('└'), "{}", left[card + 7]);
        let first = card + 8;
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
                patch: Some("@@ -1 +1,2 @@\n a\n+b\n".into()),
            }],
            owner: "o".into(),
            repo: "r".into(),
            head_oid: "abc".into(),
        };
        d.set_result(Ok::<_, String>(c));
        let buf = render_buffer(100, 40, |f| d.render(f.area(), f.buffer_mut()));
        let rows = crate::testkit::buffer_rows(&buf);
        let column = crate::testkit::buffer_column(&buf, 7);
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
            rows.iter()
                .any(|r| r.matches("Loading file..").count() == 2),
            "{rows:?}"
        );
        assert_eq!(
            d.handle_blob("src/main.rs", Ok::<_, String>(Blob::Text("a\nb\n".into()))),
            None,
            "nothing else to request"
        );
        let rows = render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows.iter().any(|r| r.matches("1  a").count() == 2)
                && rows.iter().any(|r| r.contains("2 +b")),
            "{rows:?}"
        );
        d.handle_key(KeyModifiers::NONE, KeyCode::Tab);
        assert!(
            matches!(&d.content, Content::Loaded { focus: 0, files, .. } if files.focus() == crate::view::dialog::files::Panel::Old),
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
    /// TU-R-066, TU-E-027 — a body line wider than the card wraps; without timeline items one box reads `No activity`; a deleted author reads `ghost`.
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
        assert!(rows.iter().any(|r| r.contains("by @ghost")), "{rows:?}");
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

        let rows = render_rows(80, 13, |f| d.render(f.area(), f.buffer_mut()));
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
}
