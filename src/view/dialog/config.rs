//! The configuration dialog: two sections, each a kind selection plus that kind's fields,
//! credentials included. Produces a [`BoardForm`] and [`RemoteForm`] on confirm.

use std::num::NonZeroU64;

use crossterm::event::{KeyCode, KeyModifiers};
use ferrowl_ui::state::{
    InputFieldState, InputFieldStateBuilder, SelectionState, SelectionStateBuilder,
};
use ferrowl_ui::style::{InputFieldStyle, SelectionStyle};
use ferrowl_ui::traits::{HandleEvents, SetFocus, ToLabel};
use ferrowl_ui::widgets::{InputField, InputFieldBuilder, Selection, SelectionBuilder, Widget};
use ferrowl_ui::{Border, COLOR_SCHEME};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, HorizontalAlignment, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Clear, Paragraph, StatefulWidget, Widget as RenderWidget};

use crate::config::{Board, Kind, Origin, Profile, Remote, Section, Settings, UserConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardKind {
    Github,
    Jira,
}

impl ToLabel for BoardKind {
    fn to_label(&self) -> String {
        match self {
            BoardKind::Github => "GitHub",
            BoardKind::Jira => "Jira",
        }
        .to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKind {
    Github,
    Bitbucket,
}

impl ToLabel for RemoteKind {
    fn to_label(&self) -> String {
        match self {
            RemoteKind::Github => "GitHub",
            RemoteKind::Bitbucket => "Bitbucket",
        }
        .to_string()
    }
}

/// One input field of the dialog. Order is display order within its section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    BoardOwner,
    BoardRepo,
    BoardProject,
    BoardToken,
    JiraBaseUrl,
    JiraEmail,
    JiraToken,
    JiraProjectKey,
    RemoteOwner,
    RemoteRepo,
    RemoteToken,
    BbWorkspace,
    BbRepo,
    BbUsername,
    BbAppPassword,
}

impl Field {
    pub const ALL: [Field; 15] = [
        Field::BoardOwner,
        Field::BoardRepo,
        Field::BoardProject,
        Field::BoardToken,
        Field::JiraBaseUrl,
        Field::JiraEmail,
        Field::JiraToken,
        Field::JiraProjectKey,
        Field::RemoteOwner,
        Field::RemoteRepo,
        Field::RemoteToken,
        Field::BbWorkspace,
        Field::BbRepo,
        Field::BbUsername,
        Field::BbAppPassword,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Field::BoardOwner | Field::RemoteOwner => "Owner",
            Field::BoardRepo | Field::RemoteRepo => "Repository",
            Field::BoardProject => "Project number",
            Field::BoardToken | Field::RemoteToken => "Token",
            Field::JiraBaseUrl => "Base URL",
            Field::JiraEmail => "Email",
            Field::JiraToken => "API token",
            Field::JiraProjectKey => "Project key",
            Field::BbWorkspace => "Workspace",
            Field::BbRepo => "Repository slug",
            Field::BbUsername => "Username",
            Field::BbAppPassword => "App password",
        }
    }

    fn index(self) -> usize {
        Field::ALL
            .iter()
            .position(|f| *f == self)
            .expect("every field is listed in ALL")
    }

    fn for_board(kind: BoardKind) -> &'static [Field] {
        match kind {
            BoardKind::Github => &[
                Field::BoardOwner,
                Field::BoardRepo,
                Field::BoardProject,
                Field::BoardToken,
            ],
            BoardKind::Jira => &[
                Field::JiraBaseUrl,
                Field::JiraEmail,
                Field::JiraToken,
                Field::JiraProjectKey,
            ],
        }
    }

    fn for_remote(kind: RemoteKind) -> &'static [Field] {
        match kind {
            RemoteKind::Github => &[Field::RemoteOwner, Field::RemoteRepo, Field::RemoteToken],
            RemoteKind::Bitbucket => &[
                Field::BbWorkspace,
                Field::BbRepo,
                Field::BbUsername,
                Field::BbAppPassword,
            ],
        }
    }

    /// The origin-derived hint for this field, when the origin's host matches the field's kind.
    fn hint(self, origin: &Origin) -> Option<&str> {
        match (self, origin.host) {
            (Field::BoardOwner | Field::RemoteOwner, Kind::Github) => Some(&origin.owner),
            (Field::BoardRepo | Field::RemoteRepo, Kind::Github) => Some(&origin.repo),
            (Field::BbWorkspace, Kind::Bitbucket) => Some(&origin.owner),
            (Field::BbRepo, Kind::Bitbucket) => Some(&origin.repo),
            _ => None,
        }
    }
}

/// A focusable widget of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    BoardKind,
    RemoteKind,
    Input(Field),
}

/// The Task Board section as entered, credentials included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardForm {
    Github {
        owner: String,
        repo: String,
        project: NonZeroU64,
        token: String,
    },
    Jira {
        base_url: String,
        email: String,
        token: String,
        project_key: String,
    },
}

impl BoardForm {
    /// The schema section, without a profile reference.
    pub fn section(&self) -> Board {
        match self {
            BoardForm::Github {
                owner,
                repo,
                project,
                ..
            } => Board::Github {
                credentials: None,
                owner: owner.clone(),
                repo: repo.clone(),
                project: *project,
            },
            BoardForm::Jira { project_key, .. } => Board::Jira {
                credentials: None,
                project_key: project_key.clone(),
            },
        }
    }

    pub fn profile(&self) -> Profile {
        match self {
            BoardForm::Github { token, .. } => Profile::Github {
                token: token.clone(),
            },
            BoardForm::Jira {
                base_url,
                email,
                token,
                ..
            } => Profile::Jira {
                base_url: base_url.clone(),
                email: email.clone(),
                token: token.clone(),
            },
        }
    }
}

/// The Git Remote section as entered, credentials included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteForm {
    Github {
        owner: String,
        repo: String,
        token: String,
    },
    Bitbucket {
        workspace: String,
        repo: String,
        username: String,
        app_password: String,
    },
}

impl RemoteForm {
    pub fn section(&self) -> Remote {
        match self {
            RemoteForm::Github { owner, repo, .. } => Remote::Github {
                credentials: None,
                owner: owner.clone(),
                repo: repo.clone(),
            },
            RemoteForm::Bitbucket {
                workspace, repo, ..
            } => Remote::Bitbucket {
                credentials: None,
                workspace: workspace.clone(),
                repo: repo.clone(),
            },
        }
    }

    pub fn profile(&self) -> Profile {
        match self {
            RemoteForm::Github { token, .. } => Profile::Github {
                token: token.clone(),
            },
            RemoteForm::Bitbucket {
                username,
                app_password,
                ..
            } => Profile::Bitbucket {
                username: username.clone(),
                app_password: app_password.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogEvent {
    Consumed,
    Confirm(BoardForm, RemoteForm),
    Cancel,
}

type InputWidget = Widget<InputFieldState, InputField<String>>;

const WIDTH: u16 = 84;
const HEIGHT: u16 = 21;
const KEYS: &str = "Tab/Shift+Tab: next/previous   Ctrl+F: use hint   Enter: confirm   Esc: cancel";

pub struct ConfigDialog {
    board_kind: Widget<SelectionState<BoardKind>, Selection<BoardKind>>,
    remote_kind: Widget<SelectionState<RemoteKind>, Selection<RemoteKind>>,
    fields: Vec<InputWidget>,
    focus: Slot,
    error: Option<String>,
}

impl ConfigDialog {
    /// An empty dialog; `origin` supplies owner/repo placeholders.
    pub fn new(origin: Option<&Origin>) -> ConfigDialog {
        let fields = Field::ALL
            .iter()
            .map(|field| {
                let hint = origin.and_then(|o| field.hint(o)).map(str::to_string);
                input(field.title(), hint, *field == Field::BoardProject)
            })
            .collect();
        let mut dialog = ConfigDialog {
            board_kind: selection("Task Board", vec![BoardKind::Github, BoardKind::Jira]),
            remote_kind: selection(
                "Git Remote",
                vec![RemoteKind::Github, RemoteKind::Bitbucket],
            ),
            fields,
            focus: Slot::BoardKind,
            error: None,
        };
        dialog.set_focus(Slot::BoardKind);
        dialog
    }

    /// A dialog with `settings` loaded as editable values, credentials looked up in `user`.
    pub fn from_settings(
        settings: &Settings,
        user: &UserConfig,
        origin: Option<&Origin>,
    ) -> ConfigDialog {
        let mut dialog = ConfigDialog::new(origin);
        let profile = |section: &dyn Section| {
            section
                .credentials()
                .and_then(|name| user.credentials.get(name))
        };
        match &settings.board {
            Board::Github {
                owner,
                repo,
                project,
                ..
            } => {
                dialog.board_kind.state.set_selection(0);
                dialog.set_value(Field::BoardOwner, owner);
                dialog.set_value(Field::BoardRepo, repo);
                dialog.set_value(Field::BoardProject, &project.to_string());
                if let Some(Profile::Github { token }) = profile(&settings.board) {
                    dialog.set_value(Field::BoardToken, token);
                }
            }
            Board::Jira { project_key, .. } => {
                dialog.board_kind.state.set_selection(1);
                dialog.set_value(Field::JiraProjectKey, project_key);
                if let Some(Profile::Jira {
                    base_url,
                    email,
                    token,
                }) = profile(&settings.board)
                {
                    dialog.set_value(Field::JiraBaseUrl, base_url);
                    dialog.set_value(Field::JiraEmail, email);
                    dialog.set_value(Field::JiraToken, token);
                }
            }
        }
        match &settings.remote {
            Remote::Github { owner, repo, .. } => {
                dialog.remote_kind.state.set_selection(0);
                dialog.set_value(Field::RemoteOwner, owner);
                dialog.set_value(Field::RemoteRepo, repo);
                if let Some(Profile::Github { token }) = profile(&settings.remote) {
                    dialog.set_value(Field::RemoteToken, token);
                }
            }
            Remote::Bitbucket {
                workspace, repo, ..
            } => {
                dialog.remote_kind.state.set_selection(1);
                dialog.set_value(Field::BbWorkspace, workspace);
                dialog.set_value(Field::BbRepo, repo);
                if let Some(Profile::Bitbucket {
                    username,
                    app_password,
                }) = profile(&settings.remote)
                {
                    dialog.set_value(Field::BbUsername, username);
                    dialog.set_value(Field::BbAppPassword, app_password);
                }
            }
        }
        dialog
    }

    pub fn focus(&self) -> Slot {
        self.focus
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn set_error(&mut self, message: String) {
        self.error = Some(message);
    }

    pub fn board_kind(&self) -> BoardKind {
        self.board_kind.state.values()[self.board_kind.state.selection()]
    }

    pub fn remote_kind(&self) -> RemoteKind {
        self.remote_kind.state.values()[self.remote_kind.state.selection()]
    }

    pub fn value(&self, field: Field) -> &str {
        self.fields[field.index()].state.input()
    }

    #[cfg(test)]
    pub fn placeholder(&self, field: Field) -> Option<&str> {
        self.fields[field.index()].state.placeholder().as_deref()
    }

    /// Programmatic fill, cursor at the end.
    pub fn set_value(&mut self, field: Field, value: &str) {
        let state = &mut self.fields[field.index()].state;
        state.set_input(value.to_string());
        state.set_cursor(value.chars().count());
    }

    /// Focus order: board kind, the board kind's fields, remote kind, the remote kind's fields.
    pub fn visible_slots(&self) -> Vec<Slot> {
        let mut slots = vec![Slot::BoardKind];
        slots.extend(
            Field::for_board(self.board_kind())
                .iter()
                .map(|f| Slot::Input(*f)),
        );
        slots.push(Slot::RemoteKind);
        slots.extend(
            Field::for_remote(self.remote_kind())
                .iter()
                .map(|f| Slot::Input(*f)),
        );
        slots
    }

    pub fn focus_cycle(&mut self, forward: bool) {
        let slots = self.visible_slots();
        let current = slots.iter().position(|s| *s == self.focus()).unwrap_or(0);
        let next = if forward {
            (current + 1) % slots.len()
        } else {
            (current + slots.len() - 1) % slots.len()
        };
        self.set_focus(slots[next]);
    }

    fn set_focus(&mut self, slot: Slot) {
        SetFocus::set_focused(&mut self.board_kind, slot == Slot::BoardKind);
        SetFocus::set_focused(&mut self.remote_kind, slot == Slot::RemoteKind);
        for (field, widget) in Field::ALL.iter().zip(self.fields.iter_mut()) {
            SetFocus::set_focused(widget, slot == Slot::Input(*field));
        }
        self.focus = slot;
    }

    pub fn handle_key(&mut self, modifiers: KeyModifiers, code: KeyCode) -> DialogEvent {
        match (modifiers, code) {
            (_, KeyCode::Tab) => {
                self.focus_cycle(true);
                DialogEvent::Consumed
            }
            (_, KeyCode::BackTab) => {
                self.focus_cycle(false);
                DialogEvent::Consumed
            }
            (KeyModifiers::NONE, KeyCode::Enter) => match self.forms() {
                Ok((board, remote)) => {
                    self.error = None;
                    DialogEvent::Confirm(board, remote)
                }
                Err((field, message)) => {
                    self.set_focus(Slot::Input(field));
                    self.error = Some(message);
                    DialogEvent::Consumed
                }
            },
            (KeyModifiers::NONE, KeyCode::Esc) => DialogEvent::Cancel,
            _ => {
                match self.focus {
                    Slot::BoardKind => {
                        self.board_kind.handle_events(modifiers, code);
                    }
                    Slot::RemoteKind => {
                        self.remote_kind.handle_events(modifiers, code);
                    }
                    Slot::Input(field) => {
                        self.fields[field.index()].handle_events(modifiers, code);
                    }
                }
                DialogEvent::Consumed
            }
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let [_, hcenter, _] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(WIDTH.min(area.width)),
            Constraint::Fill(1),
        ])
        .areas(area);
        let [_, boxed, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(HEIGHT.min(area.height)),
            Constraint::Fill(1),
        ])
        .areas(hcenter);
        let base = Style::default().fg(COLOR_SCHEME.text).bg(COLOR_SCHEME.bg);
        Clear.render(boxed, buf);
        buf.set_style(boxed, base);
        let block = Block::bordered()
            .style(Style::default().fg(COLOR_SCHEME.hi).bg(COLOR_SCHEME.bg))
            .title("Configuration")
            .title_alignment(HorizontalAlignment::Center);
        let inner = block.inner(boxed).inner(Margin::new(1, 0));
        block.render(boxed, buf);

        let [columns, error, keys] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        let [left, _, right] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .areas(columns);

        let board_fields = Field::for_board(self.board_kind());
        let remote_fields = Field::for_remote(self.remote_kind());
        let rows = |n: usize| {
            let mut c = vec![Constraint::Length(3)];
            c.extend(std::iter::repeat_n(Constraint::Length(3), n));
            c.push(Constraint::Fill(1));
            c
        };
        let left_rows = Layout::vertical(rows(board_fields.len())).split(left);
        StatefulWidget::render(
            &self.board_kind.widget,
            left_rows[0],
            buf,
            &mut self.board_kind.state,
        );
        for (i, field) in board_fields.iter().enumerate() {
            let w = &mut self.fields[field.index()];
            StatefulWidget::render(&w.widget, left_rows[i + 1], buf, &mut w.state);
        }
        let right_rows = Layout::vertical(rows(remote_fields.len())).split(right);
        StatefulWidget::render(
            &self.remote_kind.widget,
            right_rows[0],
            buf,
            &mut self.remote_kind.state,
        );
        for (i, field) in remote_fields.iter().enumerate() {
            let w = &mut self.fields[field.index()];
            StatefulWidget::render(&w.widget, right_rows[i + 1], buf, &mut w.state);
        }

        if let Some(message) = self.error() {
            Paragraph::new(message)
                .style(Style::default().fg(COLOR_SCHEME.error).bg(COLOR_SCHEME.bg))
                .render(error, buf);
        }
        Paragraph::new(KEYS)
            .style(
                Style::default()
                    .fg(COLOR_SCHEME.placeholder)
                    .bg(COLOR_SCHEME.bg),
            )
            .render(keys, buf);
    }

    /// The forms, or the first visible field that blocks confirming and why.
    fn forms(&self) -> Result<(BoardForm, RemoteForm), (Field, String)> {
        let need = |field: Field| -> Result<String, (Field, String)> {
            let value = self.value(field);
            if value.is_empty() {
                Err((field, format!("{} is required", field.title())))
            } else {
                Ok(value.to_string())
            }
        };
        let board = match self.board_kind() {
            BoardKind::Github => {
                let owner = need(Field::BoardOwner)?;
                let repo = need(Field::BoardRepo)?;
                let project = need(Field::BoardProject)?
                    .parse::<NonZeroU64>()
                    .map_err(|_| {
                        (
                            Field::BoardProject,
                            "Project number must be a positive integer".to_string(),
                        )
                    })?;
                let token = need(Field::BoardToken)?;
                BoardForm::Github {
                    owner,
                    repo,
                    project,
                    token,
                }
            }
            BoardKind::Jira => BoardForm::Jira {
                base_url: need(Field::JiraBaseUrl)?,
                email: need(Field::JiraEmail)?,
                token: need(Field::JiraToken)?,
                project_key: need(Field::JiraProjectKey)?,
            },
        };
        let remote = match self.remote_kind() {
            RemoteKind::Github => RemoteForm::Github {
                owner: need(Field::RemoteOwner)?,
                repo: need(Field::RemoteRepo)?,
                token: need(Field::RemoteToken)?,
            },
            RemoteKind::Bitbucket => RemoteForm::Bitbucket {
                workspace: need(Field::BbWorkspace)?,
                repo: need(Field::BbRepo)?,
                username: need(Field::BbUsername)?,
                app_password: need(Field::BbAppPassword)?,
            },
        };
        Ok((board, remote))
    }
}

fn input(title: &str, hint: Option<String>, digits_only: bool) -> InputWidget {
    let mut state = InputFieldStateBuilder::default();
    state
        .focused(false)
        .placeholder(hint.clone())
        .autofill(hint);
    if digits_only {
        state.allowed_for::<u64>();
    }
    Widget {
        state: state
            .build()
            .expect("InputFieldStateBuilder fields all default"),
        widget: InputFieldBuilder::default()
            .border(Border::Full(Margin::new(1, 0)))
            .title(Some(title.into()))
            .style(InputFieldStyle {
                border: Style::default().fg(COLOR_SCHEME.border).bg(COLOR_SCHEME.bg),
                ..InputFieldStyle::default()
            })
            .build()
            .expect("InputFieldBuilder fields all default"),
    }
}

fn selection<T: ToLabel + Clone>(
    title: &str,
    values: Vec<T>,
) -> Widget<SelectionState<T>, Selection<T>> {
    Widget {
        state: SelectionStateBuilder::default()
            .focused(false)
            .values(values)
            .build()
            .expect("values are set"),
        widget: SelectionBuilder::default()
            .border(Border::Full(Margin::new(1, 0)))
            .title(Some(title.into()))
            .style(SelectionStyle::default())
            .build()
            .expect("SelectionBuilder fields all default"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Board, Remote, Source};

    fn key(d: &mut ConfigDialog, code: KeyCode) -> DialogEvent {
        d.handle_key(KeyModifiers::NONE, code)
    }

    fn type_str(d: &mut ConfigDialog, s: &str) {
        for c in s.chars() {
            key(d, KeyCode::Char(c));
        }
    }

    fn tab(d: &mut ConfigDialog) {
        key(d, KeyCode::Tab);
    }

    fn github_origin() -> Origin {
        Origin {
            host: Kind::Github,
            owner: "TumbleOwlee".into(),
            repo: "prodgy".into(),
        }
    }

    /// Fills every visible field of a fresh GitHub/GitHub dialog.
    fn fill_github(d: &mut ConfigDialog) {
        tab(d);
        type_str(d, "o");
        tab(d);
        type_str(d, "r");
        tab(d);
        type_str(d, "3");
        tab(d);
        type_str(d, "tok");
        tab(d); // remote kind
        tab(d);
        type_str(d, "o2");
        tab(d);
        type_str(d, "r2");
        tab(d);
        type_str(d, "tok2");
    }

    #[test]
    /// TU-R-007 — both selections offer their two kinds, GitHub first.
    fn ut_new_dialog_defaults_to_github() {
        let d = ConfigDialog::new(None);
        assert_eq!(d.board_kind(), BoardKind::Github);
        assert_eq!(d.remote_kind(), RemoteKind::Github);
        assert_eq!(d.focus(), Slot::BoardKind);
        assert_eq!(BoardKind::Jira.to_label(), "Jira");
        assert_eq!(RemoteKind::Bitbucket.to_label(), "Bitbucket");
    }

    #[test]
    /// TU-R-008, TU-R-011 — visible slots follow the selected kinds in display order.
    fn ut_visible_slots_follow_kinds() {
        let mut d = ConfigDialog::new(None);
        assert_eq!(
            d.visible_slots(),
            vec![
                Slot::BoardKind,
                Slot::Input(Field::BoardOwner),
                Slot::Input(Field::BoardRepo),
                Slot::Input(Field::BoardProject),
                Slot::Input(Field::BoardToken),
                Slot::RemoteKind,
                Slot::Input(Field::RemoteOwner),
                Slot::Input(Field::RemoteRepo),
                Slot::Input(Field::RemoteToken),
            ]
        );
        key(&mut d, KeyCode::Down); // board -> Jira
        assert_eq!(d.board_kind(), BoardKind::Jira);
        let slots = d.visible_slots();
        assert_eq!(
            &slots[..5],
            &[
                Slot::BoardKind,
                Slot::Input(Field::JiraBaseUrl),
                Slot::Input(Field::JiraEmail),
                Slot::Input(Field::JiraToken),
                Slot::Input(Field::JiraProjectKey),
            ]
        );
        for _ in 0..5 {
            tab(&mut d);
        }
        assert_eq!(d.focus(), Slot::RemoteKind);
        key(&mut d, KeyCode::Char('j')); // remote -> Bitbucket
        assert_eq!(d.remote_kind(), RemoteKind::Bitbucket);
        let slots = d.visible_slots();
        assert_eq!(
            &slots[5..],
            &[
                Slot::RemoteKind,
                Slot::Input(Field::BbWorkspace),
                Slot::Input(Field::BbRepo),
                Slot::Input(Field::BbUsername),
                Slot::Input(Field::BbAppPassword),
            ]
        );
    }

    #[test]
    /// TU-R-011 — Tab and Shift+Tab cycle through visible slots and wrap.
    fn ut_tab_cycles_and_wraps() {
        let mut d = ConfigDialog::new(None);
        let slots = d.visible_slots();
        for expected in slots.iter().skip(1) {
            tab(&mut d);
            assert_eq!(d.focus(), *expected);
        }
        tab(&mut d);
        assert_eq!(d.focus(), Slot::BoardKind);
        key(&mut d, KeyCode::BackTab);
        assert_eq!(d.focus(), *slots.last().expect("non-empty"));
    }

    #[test]
    /// TU-R-009 — an origin of the section's kind fills owner/repo placeholders.
    fn ut_origin_placeholders_for_matching_kind() {
        let o = github_origin();
        let d = ConfigDialog::new(Some(&o));
        assert_eq!(d.placeholder(Field::BoardOwner), Some("TumbleOwlee"));
        assert_eq!(d.placeholder(Field::BoardRepo), Some("prodgy"));
        assert_eq!(d.placeholder(Field::RemoteOwner), Some("TumbleOwlee"));
        assert_eq!(d.placeholder(Field::RemoteRepo), Some("prodgy"));
        assert_eq!(d.placeholder(Field::BbWorkspace), None);
        let bb = Origin {
            host: Kind::Bitbucket,
            owner: "acme".into(),
            repo: "svc".into(),
        };
        let d = ConfigDialog::new(Some(&bb));
        assert_eq!(d.placeholder(Field::BoardOwner), None);
        assert_eq!(d.placeholder(Field::BbWorkspace), Some("acme"));
        assert_eq!(d.placeholder(Field::BbRepo), Some("svc"));
    }

    #[test]
    /// TU-E-003 — no origin means no placeholders.
    fn ut_no_origin_no_placeholders() {
        let d = ConfigDialog::new(None);
        assert!(Field::ALL.iter().all(|f| d.placeholder(*f).is_none()));
    }

    #[test]
    /// TU-R-010 — Ctrl+F adopts the placeholder into an empty field.
    fn ut_ctrl_f_adopts_placeholder() {
        let o = github_origin();
        let mut d = ConfigDialog::new(Some(&o));
        tab(&mut d);
        assert_eq!(d.focus(), Slot::Input(Field::BoardOwner));
        assert_eq!(
            d.handle_key(KeyModifiers::CONTROL, KeyCode::Char('f')),
            DialogEvent::Consumed
        );
        assert_eq!(d.value(Field::BoardOwner), "TumbleOwlee");
    }

    #[test]
    /// TU-R-012, TU-E-002 — Enter with every visible field filled confirms, from a field too.
    fn ut_enter_confirms_when_complete() {
        let mut d = ConfigDialog::new(None);
        fill_github(&mut d);
        assert!(matches!(d.focus(), Slot::Input(Field::RemoteToken)));
        let ev = key(&mut d, KeyCode::Enter);
        assert_eq!(
            ev,
            DialogEvent::Confirm(
                BoardForm::Github {
                    owner: "o".into(),
                    repo: "r".into(),
                    project: NonZeroU64::new(3).expect("nz"),
                    token: "tok".into()
                },
                RemoteForm::Github {
                    owner: "o2".into(),
                    repo: "r2".into(),
                    token: "tok2".into()
                }
            )
        );
    }

    #[test]
    /// TU-R-013 — Enter with an empty visible field focuses the first empty one instead.
    fn ut_enter_focuses_first_empty_field() {
        let mut d = ConfigDialog::new(None);
        assert_eq!(key(&mut d, KeyCode::Enter), DialogEvent::Consumed);
        assert_eq!(d.focus(), Slot::Input(Field::BoardOwner));
        assert!(d.error().is_some());
        type_str(&mut d, "o");
        tab(&mut d);
        type_str(&mut d, "r");
        for _ in 0..3 {
            tab(&mut d);
        }
        assert_eq!(d.focus(), Slot::RemoteKind);
        assert_eq!(key(&mut d, KeyCode::Enter), DialogEvent::Consumed);
        assert_eq!(d.focus(), Slot::Input(Field::BoardProject));
    }

    #[test]
    /// TU-R-013, CF-R-016 — a non-positive project number blocks like an empty field.
    fn ut_enter_rejects_zero_project() {
        let mut d = ConfigDialog::new(None);
        fill_github(&mut d);
        for _ in 0..5 {
            key(&mut d, KeyCode::BackTab);
        }
        assert_eq!(d.focus(), Slot::Input(Field::BoardProject));
        key(&mut d, KeyCode::Backspace);
        type_str(&mut d, "0x");
        assert_eq!(d.value(Field::BoardProject), "0");
        tab(&mut d);
        assert_eq!(key(&mut d, KeyCode::Enter), DialogEvent::Consumed);
        assert_eq!(d.focus(), Slot::Input(Field::BoardProject));
    }

    #[test]
    /// TU-R-015, TU-R-016 — Esc reports cancel; the caller decides between close and quit.
    fn ut_esc_is_cancel() {
        let mut d = ConfigDialog::new(None);
        assert_eq!(key(&mut d, KeyCode::Esc), DialogEvent::Cancel);
    }

    #[test]
    /// TU-E-001 — values typed under one kind survive switching away and back.
    fn ut_hidden_fields_keep_values() {
        let mut d = ConfigDialog::new(None);
        tab(&mut d);
        type_str(&mut d, "keep");
        key(&mut d, KeyCode::BackTab);
        key(&mut d, KeyCode::Down); // Jira
        assert!(!d.visible_slots().contains(&Slot::Input(Field::BoardOwner)));
        key(&mut d, KeyCode::Down); // GitHub again
        assert_eq!(d.value(Field::BoardOwner), "keep");
    }

    #[test]
    /// TU-R-006 — settings load as editable values, credentials from the referenced profile.
    fn ut_from_settings_prefills_values() {
        let mut user = UserConfig::default();
        user.credentials.insert(
            "j".into(),
            Profile::Jira {
                base_url: "https://x".into(),
                email: "e".into(),
                token: "t".into(),
            },
        );
        user.credentials.insert(
            "bb".into(),
            Profile::Bitbucket {
                username: "u".into(),
                app_password: "p".into(),
            },
        );
        let settings = Settings {
            board: Board::Jira {
                credentials: Some("j".into()),
                project_key: "ACME".into(),
            },
            remote: Remote::Bitbucket {
                credentials: Some("bb".into()),
                workspace: "w".into(),
                repo: "s".into(),
            },
            source: Source::UserFile,
        };
        let d = ConfigDialog::from_settings(&settings, &user, None);
        assert_eq!(d.board_kind(), BoardKind::Jira);
        assert_eq!(d.remote_kind(), RemoteKind::Bitbucket);
        assert_eq!(d.value(Field::JiraBaseUrl), "https://x");
        assert_eq!(d.value(Field::JiraEmail), "e");
        assert_eq!(d.value(Field::JiraToken), "t");
        assert_eq!(d.value(Field::JiraProjectKey), "ACME");
        assert_eq!(d.value(Field::BbWorkspace), "w");
        assert_eq!(d.value(Field::BbRepo), "s");
        assert_eq!(d.value(Field::BbUsername), "u");
        assert_eq!(d.value(Field::BbAppPassword), "p");
        let mut d = d;
        assert!(matches!(
            key(&mut d, KeyCode::Enter),
            DialogEvent::Confirm(..)
        ));
    }

    #[test]
    /// TU-R-006, CF-R-009 — GitHub settings without a profile prefill identifiers only.
    fn ut_from_settings_github_without_profile() {
        let settings = Settings {
            board: Board::Github {
                credentials: None,
                owner: "o".into(),
                repo: "r".into(),
                project: NonZeroU64::new(5).expect("nz"),
            },
            remote: Remote::Github {
                credentials: None,
                owner: "o".into(),
                repo: "r".into(),
            },
            source: Source::RepoFile,
        };
        let d = ConfigDialog::from_settings(&settings, &UserConfig::default(), None);
        assert_eq!(d.value(Field::BoardOwner), "o");
        assert_eq!(d.value(Field::BoardProject), "5");
        assert_eq!(d.value(Field::BoardToken), "");
        assert_eq!(d.value(Field::RemoteToken), "");
    }

    #[test]
    /// CF-R-029 — forms map to schema sections and profiles.
    fn ut_forms_map_to_sections_and_profiles() {
        let b = BoardForm::Jira {
            base_url: "u".into(),
            email: "e".into(),
            token: "t".into(),
            project_key: "K".into(),
        };
        assert_eq!(
            b.section(),
            Board::Jira {
                credentials: None,
                project_key: "K".into()
            }
        );
        assert_eq!(
            b.profile(),
            Profile::Jira {
                base_url: "u".into(),
                email: "e".into(),
                token: "t".into()
            }
        );
        let r = RemoteForm::Bitbucket {
            workspace: "w".into(),
            repo: "s".into(),
            username: "n".into(),
            app_password: "p".into(),
        };
        assert_eq!(
            r.section(),
            Remote::Bitbucket {
                credentials: None,
                workspace: "w".into(),
                repo: "s".into()
            }
        );
        assert_eq!(
            r.profile(),
            Profile::Bitbucket {
                username: "n".into(),
                app_password: "p".into()
            }
        );
        let g = RemoteForm::Github {
            owner: "o".into(),
            repo: "r".into(),
            token: "t".into(),
        };
        assert_eq!(g.profile(), Profile::Github { token: "t".into() });
        let gb = BoardForm::Github {
            owner: "o".into(),
            repo: "r".into(),
            project: NonZeroU64::new(1).expect("nz"),
            token: "t".into(),
        };
        assert_eq!(gb.profile(), Profile::Github { token: "t".into() });
        assert!(matches!(
            gb.section(),
            Board::Github {
                credentials: None,
                ..
            }
        ));
    }

    #[test]
    /// TU-R-007, TU-R-008, TU-R-017 — the dialog renders both sections, visible fields, and the error.
    fn ut_render_shows_sections_fields_and_error() {
        let mut d = ConfigDialog::new(None);
        d.set_error("boom".into());
        let rows = crate::testkit::render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        let joined = rows.join("\n");
        for needle in [
            "Task Board",
            "Git Remote",
            "GitHub",
            "Owner",
            "Project",
            "Token",
            "boom",
        ] {
            assert!(joined.contains(needle), "missing {needle}:\n{joined}");
        }
        assert!(
            !joined.contains("Workspace"),
            "hidden field rendered:\n{joined}"
        );
    }

    #[test]
    /// TU-R-037 — a kind selection shows only the selected kind on one line; Down cycles it.
    fn ut_kind_selection_is_single_line() {
        let mut d = ConfigDialog::new(None);
        let rows = crate::testkit::render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        let joined = rows.join("\n");
        assert!(
            !joined.contains("Jira") && !joined.contains("Bitbucket"),
            "{joined}"
        );
        let title_row = rows
            .iter()
            .position(|r| r.contains("Task Board"))
            .expect("title row");
        assert!(
            rows[title_row + 1].contains("GitHub"),
            "{}",
            rows[title_row + 1]
        );
        assert!(rows[title_row + 2].contains('─'), "{}", rows[title_row + 2]);
        key(&mut d, KeyCode::Down);
        let rows = crate::testkit::render_rows(100, 30, |f| d.render(f.area(), f.buffer_mut()));
        assert!(
            rows[title_row + 1].contains("Jira"),
            "{}",
            rows[title_row + 1]
        );
        assert!(!rows.join("\n").contains("GitHub") || d.remote_kind() == RemoteKind::Github);
    }
}
