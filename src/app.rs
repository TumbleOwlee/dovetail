//! Application state: what is held, which layer has the keyboard, and the frame layout.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::command::{self, Cmd};
use crate::config::profile::{profile_base, store_profile};
use crate::config::{
    Board, ConfigError, Origin, Profile, Remote, Section, Settings, Source, UserConfig, paths,
    store,
};
use crate::event::Message;
use crate::view::board::{self, BoardView};
use crate::view::command_line::{CommandLine, CommandLineEvent};
use crate::view::dialog::config::{BoardForm, ConfigDialog, DialogEvent, RemoteForm};
use crate::view::dialog::config::{Choice, Field};
use crate::view::dialog::details::{BlobRef, DetailsDialog, DetailsEvent, Link};
use crate::view::dialog::{issue, pull};
use crate::view::notice;
use crate::view::remote::RemoteView;
use crate::view::tabs::{self, Tab};

/// A fetch the loop runs on the app's behalf, keyed by the credentials it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchRequest {
    GithubProjects {
        token: String,
        owner: String,
    },
    JiraProjects {
        base_url: String,
        email: String,
        token: String,
    },
    Board {
        token: String,
        owner: String,
        number: u64,
    },
    Issue {
        token: String,
        id: String,
    },
    PullRequests {
        token: String,
        owner: String,
        repo: String,
    },
    PullRequest {
        token: String,
        owner: String,
        repo: String,
        number: u64,
    },
    Blob {
        token: String,
        owner: String,
        repo: String,
        oid: String,
        path: String,
    },
}

/// What the Task Board tab body shows.
pub enum BoardState {
    /// No request is possible: the tab shows the configuration summary.
    Unavailable,
    Loading,
    Failed(String),
    Loaded(BoardView),
}

/// What the Git Remote tab body shows.
pub enum RemoteState {
    /// No request is possible: the tab shows the configuration summary.
    Unavailable,
    Loading,
    Failed(String),
    Loaded(Box<RemoteView>),
}

pub struct App {
    pub repo_root: PathBuf,
    pub user_path: PathBuf,
    pub user_config: UserConfig,
    pub settings: Option<Settings>,
    pub origin: Option<Origin>,
    pub active_tab: Tab,
    pub dialog: Option<ConfigDialog>,
    /// The issue details overlay while open.
    pub issue: Option<DetailsDialog>,
    /// The pull request details overlay while open.
    pub pull: Option<DetailsDialog>,
    pub command_line: CommandLine,
    pub board: BoardState,
    pub remote: RemoteState,
    pending_fetches: Vec<FetchRequest>,
    /// A dialog built by `config` that opens once its project list arrives.
    waiting_dialog: Option<ConfigDialog>,
    /// Ctrl+T was pressed; the next key selects a tab.
    tab_prefix: bool,
    quit: bool,
}

impl App {
    /// Opens the configuration dialog when `settings` is `None`.
    pub fn new(
        repo_root: PathBuf,
        user_path: PathBuf,
        user_config: UserConfig,
        settings: Option<Settings>,
        origin: Option<Origin>,
    ) -> App {
        let dialog = settings
            .is_none()
            .then(|| ConfigDialog::new(origin.as_ref()));
        let mut app = App {
            repo_root,
            user_path,
            user_config,
            settings,
            origin,
            active_tab: Tab::Board,
            dialog,
            issue: None,
            pull: None,
            command_line: CommandLine::new(),
            board: BoardState::Unavailable,
            remote: RemoteState::Unavailable,
            pending_fetches: Vec::new(),
            waiting_dialog: None,
            tab_prefix: false,
            quit: false,
        };
        app.request_board();
        app.request_remote();
        app
    }

    /// The GitHub board's owner, project number and token when all are configured.
    fn github_board(&self) -> Option<(&str, u64, &str)> {
        let settings = self.settings.as_ref()?;
        let profile = settings
            .board
            .credentials()
            .and_then(|name| self.user_config.credentials.get(name));
        match (&settings.board, profile) {
            (Board::Github { owner, project, .. }, Some(Profile::Github { token })) => {
                Some((owner, project.get(), token))
            }
            _ => None,
        }
    }

    /// Queues a board request when the board is GitHub with stored credentials.
    fn request_board(&mut self) -> bool {
        let Some((owner, number, token)) = self.github_board() else {
            self.board = BoardState::Unavailable;
            return false;
        };
        self.pending_fetches.push(FetchRequest::Board {
            token: token.to_string(),
            owner: owner.to_string(),
            number,
        });
        self.board = BoardState::Loading;
        true
    }

    /// The GitHub remote's owner, repository and token when all are configured.
    fn github_remote(&self) -> Option<(&str, &str, &str)> {
        let settings = self.settings.as_ref()?;
        let profile = settings
            .remote
            .credentials()
            .and_then(|name| self.user_config.credentials.get(name));
        match (&settings.remote, profile) {
            (Remote::Github { owner, repo, .. }, Some(Profile::Github { token })) => {
                Some((owner, repo, token))
            }
            _ => None,
        }
    }

    /// Queues a pull request list request when the remote is GitHub with stored credentials.
    fn request_remote(&mut self) -> bool {
        let Some((owner, repo, token)) = self.github_remote() else {
            self.remote = RemoteState::Unavailable;
            return false;
        };
        self.pending_fetches.push(FetchRequest::PullRequests {
            token: token.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        });
        self.remote = RemoteState::Loading;
        true
    }

    /// Opens the details overlay for the selected pull request and queues its request.
    fn open_pull(&mut self) {
        let RemoteState::Loaded(view) = &self.remote else {
            return;
        };
        let (Some(pull), Some((owner, repo, token))) = (view.selected(), self.github_remote())
        else {
            return;
        };
        let request = FetchRequest::PullRequest {
            token: token.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number: pull.number,
        };
        self.pull = Some(DetailsDialog::new(
            pull.number,
            pull.title.clone(),
            pull::LOADING,
        ));
        self.pending_fetches.push(request);
    }

    /// Opens the details overlay for the selected card and queues its request.
    fn open_issue(&mut self) {
        let BoardState::Loaded(view) = &self.board else {
            return;
        };
        let (Some(card), Some((_, _, token))) = (view.selected_card(), self.github_board()) else {
            return;
        };
        let request = FetchRequest::Issue {
            token: token.to_string(),
            id: card.id.clone(),
        };
        self.issue = Some(DetailsDialog::new(
            card.number,
            card.title.clone(),
            issue::LOADING,
        ));
        self.pending_fetches.push(request);
    }

    /// Switches to the tab of the linked item and opens its details overlay, with the GitHub
    /// token of either configured side.
    fn open_link(&mut self, link: Link) {
        let Some(token) = self
            .github_remote()
            .map(|(_, _, t)| t)
            .or_else(|| self.github_board().map(|(_, _, t)| t))
        else {
            return;
        };
        let token = token.to_string();
        match link {
            Link::Issue { id, number, title } => {
                self.active_tab = Tab::Board;
                self.issue = Some(DetailsDialog::new(number, title, issue::LOADING));
                self.pending_fetches.push(FetchRequest::Issue { token, id });
            }
            Link::Pull {
                owner,
                repo,
                number,
                title,
            } => {
                self.active_tab = Tab::Remote;
                self.pull = Some(DetailsDialog::new(number, title, pull::LOADING));
                self.pending_fetches.push(FetchRequest::PullRequest {
                    token,
                    owner,
                    repo,
                    number,
                });
            }
        }
    }

    /// Queues the file content request of the pull request overlay, with either GitHub token.
    fn request_blob(&mut self, blob: Option<BlobRef>) {
        let Some(blob) = blob else {
            return;
        };
        let Some(token) = self
            .github_remote()
            .map(|(_, _, t)| t)
            .or_else(|| self.github_board().map(|(_, _, t)| t))
        else {
            return;
        };
        self.pending_fetches.push(FetchRequest::Blob {
            token: token.to_string(),
            owner: blob.owner,
            repo: blob.repo,
            oid: blob.oid,
            path: blob.path,
        });
    }

    /// Fetches queued since the last call, for the loop to run.
    pub fn take_fetch_requests(&mut self) -> Vec<FetchRequest> {
        std::mem::take(&mut self.pending_fetches)
    }

    /// A fetch outcome: opens the waiting dialog with the list, or reports the failure.
    /// Ignored when no dialog is waiting.
    pub fn handle_message(&mut self, message: Message) {
        if let Message::Board(outcome) = message {
            self.board = match outcome {
                Ok(board) => BoardState::Loaded(BoardView::new(board)),
                Err(e) => BoardState::Failed(e.to_string()),
            };
            return;
        }
        if let Message::PullRequests(result) = message {
            self.remote = match result {
                Ok(pulls) => RemoteState::Loaded(Box::new(RemoteView::new(pulls))),
                Err(e) => RemoteState::Failed(e.to_string()),
            };
            return;
        }
        if let Message::PullRequest(result) = message {
            if let Some(dialog) = self.pull.as_mut() {
                let blob = dialog.set_result(result.map(pull::content));
                self.request_blob(blob);
            }
            return;
        }
        if let Message::Blob { path, result } = message {
            if let Some(dialog) = self.pull.as_mut() {
                let blob = dialog.handle_blob(&path, result);
                self.request_blob(blob);
            }
            return;
        }
        if let Message::Issue(result) = message {
            if let Some(dialog) = self.issue.as_mut() {
                dialog.set_result(result.map(issue::content));
            }
            return;
        }
        let Some(mut dialog) = self.waiting_dialog.take() else {
            return;
        };
        self.command_line.clear_notice();
        let outcome = match message {
            Message::GithubProjects(Ok(projects)) => Ok((
                Field::BoardProject,
                projects
                    .into_iter()
                    .map(|p| Choice {
                        value: p.number.to_string(),
                        label: p.title,
                    })
                    .collect::<Vec<_>>(),
            )),
            Message::JiraProjects(Ok(projects)) => Ok((
                Field::JiraProjectKey,
                projects
                    .into_iter()
                    .map(|p| Choice {
                        label: format!("{} {}", p.key, p.name),
                        value: p.key,
                    })
                    .collect::<Vec<_>>(),
            )),
            Message::GithubProjects(Err(e)) => Err(e.to_string()),
            Message::JiraProjects(Err(e)) => Err(e.to_string()),
            Message::Board(_)
            | Message::Issue(_)
            | Message::PullRequests(_)
            | Message::PullRequest(_)
            | Message::Blob { .. } => {
                unreachable!("handled above")
            }
        };
        match outcome {
            Ok((field, choices)) => {
                dialog.set_options(field, choices);
                self.dialog = Some(dialog);
            }
            Err(message) => self.command_line.set_error(message),
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Whether a section's profile reference resolves to a stored profile.
    pub fn credentials_present(&self, section: &dyn Section) -> bool {
        section
            .credentials()
            .is_some_and(|name| self.user_config.credentials.contains_key(name))
    }

    /// Dialog first, then the command line, then the main view.
    pub fn handle_key(&mut self, modifiers: KeyModifiers, code: KeyCode) {
        self.command_line.clear_error();
        if let Some(dialog) = self.dialog.as_mut() {
            match dialog.handle_key(modifiers, code) {
                DialogEvent::Consumed => {}
                DialogEvent::Confirm(board, remote) => self.confirm_dialog(board, remote),
                DialogEvent::Cancel => {
                    if self.settings.is_none() {
                        self.quit = true;
                    } else {
                        self.dialog = None;
                    }
                }
            }
            return;
        }
        if let Some(issue) = self.issue.as_mut() {
            match issue.handle_key(modifiers, code) {
                DetailsEvent::Consumed => {}
                DetailsEvent::Close => self.issue = None,
                DetailsEvent::Open(link) => {
                    self.issue = None;
                    self.open_link(link);
                }
                DetailsEvent::Fetch(_) => {}
            }
            return;
        }
        if let Some(pull) = self.pull.as_mut() {
            match pull.handle_key(modifiers, code) {
                DetailsEvent::Consumed => {}
                DetailsEvent::Close => self.pull = None,
                DetailsEvent::Open(link) => {
                    self.pull = None;
                    self.open_link(link);
                }
                DetailsEvent::Fetch(blob) => self.request_blob(Some(blob)),
            }
            return;
        }
        if self.command_line.is_open() {
            match self.command_line.handle_key(modifiers, code) {
                CommandLineEvent::Consumed | CommandLineEvent::Cancel => {}
                CommandLineEvent::Submit(text) => self.execute(command::parse(&text)),
            }
            return;
        }
        if std::mem::take(&mut self.tab_prefix) {
            match code {
                KeyCode::Char('l') => self.active_tab = self.active_tab.next(),
                KeyCode::Char('h') => self.active_tab = self.active_tab.previous(),
                KeyCode::Char(c) => {
                    if let Some(tab) = c.to_digit(10).and_then(|n| Tab::ALL.get(n as usize)) {
                        self.active_tab = *tab;
                    }
                }
                _ => {}
            }
            return;
        }
        match (modifiers, code) {
            (KeyModifiers::CONTROL, KeyCode::Char('t')) => self.tab_prefix = true,
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(':')) => {
                self.command_line.open();
            }
            (KeyModifiers::NONE, KeyCode::Enter) if self.active_tab == Tab::Board => {
                self.open_issue();
            }
            (KeyModifiers::NONE, code) if self.active_tab == Tab::Board => {
                if let BoardState::Loaded(view) = &mut self.board {
                    view.handle_key(code);
                }
            }
            (KeyModifiers::NONE, KeyCode::Enter) if self.active_tab == Tab::Remote => {
                self.open_pull();
            }
            (modifiers, code) if self.active_tab == Tab::Remote => {
                if let RemoteState::Loaded(view) = &mut self.remote {
                    view.handle_key(modifiers, code);
                }
            }
            _ => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let [upper, bottom] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        let [top, middle] =
            Layout::horizontal([Constraint::Length(tabs::TAB_LINE_WIDTH), Constraint::Min(1)])
                .areas(upper);
        let present = self
            .settings
            .as_ref()
            .is_some_and(|s| self.credentials_present(self.active_tab.section(s)));
        let lines = tabs::summary_lines(self.active_tab, self.settings.as_ref(), present);
        let buf = frame.buffer_mut();
        tabs::render_tab_line(top, buf, self.active_tab);
        match self.active_tab {
            Tab::Board => match &self.board {
                BoardState::Loaded(view) => view.render(middle, buf),
                BoardState::Loading => board::render_loading(middle, buf),
                BoardState::Failed(message) => notice::render_error(middle, buf, message),
                BoardState::Unavailable => tabs::render_body(middle, buf, &lines),
            },
            Tab::Remote => match &mut self.remote {
                RemoteState::Loaded(view) => view.render(middle, buf),
                RemoteState::Loading => {
                    notice::render_loading(middle, buf, "Pull requests are loading..");
                }
                RemoteState::Failed(message) => notice::render_error(middle, buf, message),
                RemoteState::Unavailable => tabs::render_body(middle, buf, &lines),
            },
        }
        self.command_line.render(bottom, buf);
        if let Some(issue) = self.issue.as_mut() {
            issue.render(area, buf);
        }
        if let Some(pull) = self.pull.as_mut() {
            pull.render(area, buf);
        }
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.render(area, buf);
        }
        self.command_line.render_overlay(area, buf);
    }

    fn execute(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Empty => {}
            Cmd::Quit => self.quit = true,
            Cmd::Config => self.open_dialog(),
            Cmd::Board => self.active_tab = Tab::Board,
            Cmd::Remote => self.active_tab = Tab::Remote,
            Cmd::Write => self.report(self.settings.is_some(), App::write_user),
            Cmd::WriteRepo => self.report(self.settings.is_some(), App::write_repo),
            Cmd::Reload => {
                let board = self.request_board();
                let remote = self.request_remote();
                if !board && !remote {
                    self.command_line.set_error("not configured".to_string());
                }
            }
            Cmd::Unknown(text) => self
                .command_line
                .set_error(format!("unknown command: {text}")),
        }
    }

    /// Runs a write when configured; any failure lands in the command line.
    fn report(&mut self, configured: bool, write: fn(&mut App) -> Result<(), ConfigError>) {
        if !configured {
            self.command_line.set_error("not configured".to_string());
            return;
        }
        if let Err(e) = write(self) {
            self.command_line.set_error(e.to_string());
        }
    }

    /// Opens the dialog at once, or queues the project fetch it must wait for.
    fn open_dialog(&mut self) {
        let Some(settings) = &self.settings else {
            self.dialog = Some(ConfigDialog::new(self.origin.as_ref()));
            return;
        };
        let dialog = ConfigDialog::from_settings(settings, &self.user_config, self.origin.as_ref());
        let profile = settings
            .board
            .credentials()
            .and_then(|name| self.user_config.credentials.get(name));
        let request = match (&settings.board, profile) {
            (Board::Github { owner, .. }, Some(Profile::Github { token })) => {
                Some(FetchRequest::GithubProjects {
                    token: token.clone(),
                    owner: owner.clone(),
                })
            }
            (
                Board::Jira { .. },
                Some(Profile::Jira {
                    base_url,
                    email,
                    token,
                }),
            ) => Some(FetchRequest::JiraProjects {
                base_url: base_url.clone(),
                email: email.clone(),
                token: token.clone(),
            }),
            _ => None,
        };
        match request {
            Some(request) => {
                self.pending_fetches.push(request);
                self.waiting_dialog = Some(dialog);
                self.command_line
                    .set_notice("loading projects…".to_string());
            }
            None => self.dialog = Some(dialog),
        }
    }

    fn confirm_dialog(&mut self, board: BoardForm, remote: RemoteForm) {
        // Work on a copy so a failed save leaves the held configuration untouched.
        let mut candidate = self.user_config.clone();
        let existing = |section: &dyn Section| section.credentials().map(str::to_string);
        let board_ref = self.settings.as_ref().and_then(|s| existing(&s.board));
        let remote_ref = self.settings.as_ref().and_then(|s| existing(&s.remote));
        let board_name = store_profile(
            &mut candidate,
            board_ref.as_deref(),
            &profile_base("board", &self.repo_root),
            board.profile(),
        );
        // Identical credential values on both sides are one profile, referenced twice.
        let remote_name = if remote.profile() == board.profile() {
            board_name.clone()
        } else {
            store_profile(
                &mut candidate,
                remote_ref.as_deref(),
                &profile_base("remote", &self.repo_root),
                remote.profile(),
            )
        };
        let mut board = board.section();
        board.set_credentials(Some(board_name));
        let mut remote = remote.section();
        remote.set_credentials(Some(remote_name));
        store::upsert_repo(
            &mut candidate,
            &self.repo_root,
            board.clone(),
            remote.clone(),
        );
        match store::save_user_config(&self.user_path, &candidate) {
            Ok(()) => {
                self.user_config = candidate;
                self.settings = Some(Settings {
                    board,
                    remote,
                    source: Source::UserFile,
                });
                self.dialog = None;
                self.request_board();
                self.request_remote();
            }
            Err(e) => {
                if let Some(dialog) = self.dialog.as_mut() {
                    dialog.set_error(e.to_string());
                }
            }
        }
    }

    fn write_user(&mut self) -> Result<(), ConfigError> {
        let Some(settings) = &self.settings else {
            return Ok(());
        };
        let mut candidate = self.user_config.clone();
        store::upsert_repo(
            &mut candidate,
            &self.repo_root,
            settings.board.clone(),
            settings.remote.clone(),
        );
        store::save_user_config(&self.user_path, &candidate)?;
        self.user_config = candidate;
        Ok(())
    }

    fn write_repo(&mut self) -> Result<(), ConfigError> {
        let Some(settings) = &self.settings else {
            return Ok(());
        };
        store::save_repo_config(
            &paths::repo_config_path(&self.repo_root),
            &settings.board,
            &settings.remote,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Board, Kind, Remote, Source, store};
    use crate::testkit::{TempDir, render_rows};
    use crate::view::dialog::config::{Field, Slot};
    use std::num::NonZeroU64;

    fn settings() -> Settings {
        Settings {
            board: Board::Github {
                credentials: Some("gh".into()),
                owner: "o".into(),
                repo: "r".into(),
                project: NonZeroU64::new(1).expect("nz"),
            },
            remote: Remote::Github {
                credentials: None,
                owner: "o".into(),
                repo: "r".into(),
            },
            source: Source::UserFile,
        }
    }

    fn user_with_gh() -> UserConfig {
        let mut u = UserConfig::default();
        u.credentials.insert(
            "gh".into(),
            crate::config::Profile::Github { token: "t".into() },
        );
        u
    }

    fn app(t: &TempDir, settings: Option<Settings>) -> App {
        let root = t.path().join("repo");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        App::new(
            root,
            t.path().join("cfg").join("config.toml"),
            user_with_gh(),
            settings,
            None,
        )
    }

    fn key(a: &mut App, code: KeyCode) {
        a.handle_key(KeyModifiers::NONE, code);
    }

    fn command(a: &mut App, text: &str) {
        key(a, KeyCode::Char(':'));
        for c in text.chars() {
            key(a, KeyCode::Char(c));
        }
        key(a, KeyCode::Enter);
    }

    fn type_str(a: &mut App, s: &str) {
        for c in s.chars() {
            key(a, KeyCode::Char(c));
        }
    }

    #[test]
    /// TU-R-005 — without settings the dialog is open at start; with settings it is not.
    fn ut_starts_with_dialog_when_unconfigured() {
        let t = TempDir::new("start");
        assert!(app(&t, None).dialog.is_some());
        assert!(app(&t, Some(settings())).dialog.is_none());
    }

    #[test]
    /// TU-R-020, TU-R-021 — Ctrl+T then l/h or a digit switches tabs; bare keys do nothing.
    fn ut_tab_keys_switch_tabs() {
        let t = TempDir::new("tabs");
        let mut a = app(&t, Some(settings()));
        let ctrl_t = |a: &mut App| a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        assert_eq!(a.active_tab, Tab::Board);
        ctrl_t(&mut a);
        key(&mut a, KeyCode::Char('l'));
        assert_eq!(a.active_tab, Tab::Remote);
        ctrl_t(&mut a);
        key(&mut a, KeyCode::Char('l'));
        assert_eq!(a.active_tab, Tab::Board);
        ctrl_t(&mut a);
        key(&mut a, KeyCode::Char('h'));
        assert_eq!(a.active_tab, Tab::Remote);
        ctrl_t(&mut a);
        key(&mut a, KeyCode::Char('0'));
        assert_eq!(a.active_tab, Tab::Board);
        ctrl_t(&mut a);
        key(&mut a, KeyCode::Char('1'));
        assert_eq!(a.active_tab, Tab::Remote);
        for code in [
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Char('0'),
            KeyCode::Char('h'),
        ] {
            key(&mut a, code);
            assert_eq!(
                a.active_tab,
                Tab::Remote,
                "{code:?} must not switch without the prefix"
            );
        }
    }

    #[test]
    /// TU-R-047, TU-E-014 — the prefix consumes exactly one key; a foreign key or a bad index disarms it.
    fn ut_tab_prefix_disarms() {
        let t = TempDir::new("prefix");
        let mut a = app(&t, Some(settings()));
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('x'));
        key(&mut a, KeyCode::Char('l'));
        assert_eq!(a.active_tab, Tab::Board);
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('9'));
        assert_eq!(a.active_tab, Tab::Board);
        key(&mut a, KeyCode::Char('l'));
        assert_eq!(a.active_tab, Tab::Board);
        // The prefix does not swallow the command line either: `:` after a foreign key opens it.
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char(':'));
        assert!(!a.command_line.is_open());
        key(&mut a, KeyCode::Char(':'));
        assert!(a.command_line.is_open());
    }

    #[test]
    /// TU-R-025, TU-R-029, TU-R-030, TU-R-031 — `:` opens the line; q, board, remote act.
    fn ut_commands_quit_and_switch() {
        let t = TempDir::new("cmds");
        let mut a = app(&t, Some(settings()));
        key(&mut a, KeyCode::Char(':'));
        assert!(a.command_line.is_open());
        key(&mut a, KeyCode::Esc);
        assert!(!a.command_line.is_open());
        command(&mut a, "remote");
        assert_eq!(a.active_tab, Tab::Remote);
        command(&mut a, "board");
        assert_eq!(a.active_tab, Tab::Board);
        assert!(!a.should_quit());
        command(&mut a, "q");
        assert!(a.should_quit());
    }

    #[test]
    /// TU-R-034 — an unknown command shows an error until the next key press.
    fn ut_unknown_command_error_until_next_key() {
        let t = TempDir::new("unknown");
        let mut a = app(&t, Some(settings()));
        command(&mut a, "frob");
        assert_eq!(a.command_line.error(), Some("unknown command: frob"));
        key(&mut a, KeyCode::Char('1'));
        assert_eq!(a.command_line.error(), None);
    }

    #[test]
    /// TU-R-003 — Ctrl+C neither quits nor changes anything.
    fn ut_ctrl_c_is_consumed() {
        let t = TempDir::new("ctrlc");
        let mut a = app(&t, Some(settings()));
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('c'));
        assert!(!a.should_quit());
        assert!(a.dialog.is_none() && !a.command_line.is_open());
    }

    #[test]
    /// TU-R-004 — an open dialog takes keys before the command line and main view.
    fn ut_dialog_takes_keys_first() {
        let t = TempDir::new("dispatch");
        let mut a = app(&t, None);
        key(&mut a, KeyCode::Char(':'));
        assert!(!a.command_line.is_open());
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.active_tab, Tab::Board);
        assert_eq!(
            a.dialog.as_ref().map(|d| d.focus()),
            Some(Slot::Input(Field::Owner))
        );
    }

    #[test]
    /// TU-R-004 — an open command line takes keys before the main view.
    fn ut_command_line_takes_keys_before_main_view() {
        let t = TempDir::new("dispatch2");
        let mut a = app(&t, Some(settings()));
        key(&mut a, KeyCode::Char(':'));
        key(&mut a, KeyCode::Char('2'));
        assert_eq!(a.active_tab, Tab::Board);
        assert_eq!(a.command_line.input(), "2");
    }

    #[test]
    /// TU-R-016 — Esc on the first-run dialog quits.
    fn ut_esc_on_first_run_quits() {
        let t = TempDir::new("firstrun");
        let mut a = app(&t, None);
        key(&mut a, KeyCode::Esc);
        assert!(a.should_quit());
    }

    #[test]
    /// TU-R-006, TU-R-015 — `:config` opens the dialog prefilled; Esc closes it, settings kept.
    fn ut_config_command_opens_and_esc_closes() {
        let t = TempDir::new("config");
        let mut a = app(&t, Some(settings()));
        command(&mut a, "config");
        assert!(a.dialog.is_none(), "waits for the project list");
        a.handle_message(Message::GithubProjects(Ok(Vec::new())));
        let d = a.dialog.as_ref().expect("dialog");
        assert_eq!(d.value(Field::Owner), "o");
        assert_eq!(d.value(Field::GithubToken), "t");
        key(&mut a, KeyCode::Esc);
        assert!(a.dialog.is_none());
        assert!(!a.should_quit());
        assert_eq!(a.settings, Some(settings()));
    }

    #[test]
    /// TU-R-014, TU-R-045, CF-R-029, CF-R-031, CF-R-036 — confirming writes the user file, applies settings, closes.
    fn ut_confirm_writes_applies_and_closes() {
        let t = TempDir::new("confirm");
        let mut a = app(&t, None);
        key(&mut a, KeyCode::Tab);
        type_str(&mut a, "own");
        key(&mut a, KeyCode::Tab);
        type_str(&mut a, "rep");
        key(&mut a, KeyCode::Tab);
        type_str(&mut a, "7");
        key(&mut a, KeyCode::Tab);
        type_str(&mut a, "tok");
        key(&mut a, KeyCode::Enter);
        assert!(a.dialog.is_none());
        let s = a.settings.as_ref().expect("settings applied");
        assert_eq!(s.source, Source::UserFile);
        assert_eq!(s.board.credentials(), Some("board-repo"));
        // CF-R-036: shared GitHub values reference the one stored profile.
        assert_eq!(s.remote.credentials(), Some("board-repo"));
        assert_eq!(s.remote.identifiers(), s.board.identifiers()[..2].to_vec());
        assert!(!a.user_config.credentials.contains_key("remote-repo"));
        assert!(a.credentials_present(&s.board));
        let on_disk = store::load_user_config(&a.user_path).expect("loads");
        assert_eq!(on_disk, a.user_config);
        assert_eq!(
            on_disk.credentials["board-repo"],
            crate::config::Profile::Github {
                token: "tok".into()
            }
        );
        assert_eq!(on_disk.repo[0].path, a.repo_root);
    }

    #[test]
    /// TU-R-017 — a failed write keeps the dialog open with the error and leaves state untouched.
    fn ut_confirm_write_failure_keeps_dialog() {
        let t = TempDir::new("confirmfail");
        let root = t.path().join("repo");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        let blocker = t.write("blocker", "");
        let mut a = App::new(
            root,
            blocker.join("config.toml"),
            UserConfig::default(),
            None,
            None,
        );
        let d = a.dialog.as_mut().expect("dialog");
        for (f, v) in [
            (Field::Owner, "o"),
            (Field::Repo, "r"),
            (Field::BoardProject, "1"),
            (Field::GithubToken, "t"),
        ] {
            d.set_value(f, v);
        }
        key(&mut a, KeyCode::Enter);
        let d = a.dialog.as_ref().expect("dialog stays open");
        assert!(d.error().is_some());
        assert!(a.settings.is_none());
        assert!(a.user_config.credentials.is_empty());
    }

    #[test]
    /// TU-R-032, CF-R-028 — `:w` writes the held settings to the user file.
    fn ut_write_user_command() {
        let t = TempDir::new("w");
        let mut a = app(&t, Some(settings()));
        command(&mut a, "w");
        assert_eq!(a.command_line.error(), None);
        let on_disk = store::load_user_config(&a.user_path).expect("loads");
        assert_eq!(on_disk.repo.len(), 1);
        assert_eq!(on_disk.repo[0].board, settings().board);
    }

    #[test]
    /// TU-R-033, CF-R-033 — `:wr` writes the repository file without credentials.
    fn ut_write_repo_command() {
        let t = TempDir::new("wr");
        let mut a = app(&t, Some(settings()));
        command(&mut a, "wr");
        assert_eq!(a.command_line.error(), None);
        let text = std::fs::read_to_string(a.repo_root.join(".prodgy.toml")).expect("written");
        assert!(
            text.contains("owner = \"o\"") && !text.contains("credentials"),
            "{text}"
        );
    }

    #[test]
    /// TU-R-035 — `:w` and `:wr` without settings report `not configured`.
    fn ut_write_without_settings_errors() {
        let t = TempDir::new("wnone");
        let mut a = app(&t, None);
        key(&mut a, KeyCode::Esc); // first-run Esc quits, but the command path is what's under test
        a.dialog = None;
        a.settings = None;
        command(&mut a, "w");
        assert_eq!(a.command_line.error(), Some("not configured"));
        command(&mut a, "wr");
        assert_eq!(a.command_line.error(), Some("not configured"));
    }

    #[test]
    /// TU-R-036, CF-R-035 — a failed `:wr` shows the error and the app keeps running.
    fn ut_write_repo_failure_shows_error() {
        let t = TempDir::new("wrfail");
        let mut a = app(&t, Some(settings()));
        std::fs::create_dir_all(a.repo_root.join(".prodgy.toml"))
            .expect("directory blocks the file");
        command(&mut a, "wr");
        assert!(a.command_line.error().is_some());
        assert!(!a.should_quit());
    }

    #[test]
    /// TU-R-018, TU-R-019, TU-R-022 — tab line on top, summary in the middle, command line at the bottom.
    fn ut_render_layout() {
        let t = TempDir::new("render");
        let mut a = app(&t, Some(settings()));
        command(&mut a, "frob");
        let buf = crate::testkit::render_buffer(60, 10, |f| a.render(f));
        let rows = crate::testkit::buffer_rows(&buf);
        let column = crate::testkit::buffer_column(&buf, 1);
        assert!(
            column.contains("0 TASK") || column.contains("TASK B"),
            "{column:?}"
        );
        assert!(
            !column.contains("frob"),
            "the tab line ends above the command line"
        );
        let (row, x) = rows
            .iter()
            .enumerate()
            .find_map(|(i, r)| r.find("Board is loading..").map(|x| (i, x)))
            .expect("loading box");
        assert!((3..=6).contains(&row), "vertically centered: {rows:?}");
        assert!((15..=25).contains(&x), "horizontally centered: {rows:?}");
        assert!(
            rows[row - 1].contains('┌') && rows[row + 1].contains('└'),
            "{rows:?}"
        );
        assert_eq!(rows[9], "unknown command: frob");
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('1'));
        let rows = render_rows(60, 10, |f| a.render(f));
        assert_eq!(&rows[0][3..], "kind: github", "body right of the tab line");
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('0'));
        key(&mut a, KeyCode::Char(':'));
        let rows = render_rows(60, 10, |f| a.render(f));
        assert_eq!(rows[9], ":");
    }

    #[test]
    /// TU-R-005, TU-R-024 — the dialog renders over the unconfigured body.
    fn ut_render_dialog_over_main_view() {
        let t = TempDir::new("renderdialog");
        let mut a = app(&t, None);
        let rows = render_rows(100, 30, |f| a.render(f));
        let joined = rows.join("\n");
        let buf = crate::testkit::render_buffer(100, 30, |f| a.render(f));
        let column = crate::testkit::buffer_column(&buf, 1);
        assert!(column.contains("0 TASK BOARD"), "{column:?}");
        assert!(joined.contains("Owner"), "{joined}");
    }

    #[test]
    /// TU-R-022 — credential presence needs a reference that resolves to a stored profile.
    fn ut_credentials_present_requires_profile() {
        let t = TempDir::new("cred");
        let a = app(&t, Some(settings()));
        let s = settings();
        assert!(a.credentials_present(&s.board));
        assert!(!a.credentials_present(&s.remote));
        let dangling = Board::Jira {
            credentials: Some("zz".into()),
            project_key: "K".into(),
        };
        assert!(!a.credentials_present(&dangling));
        assert_eq!(dangling.kind(), Kind::Jira);
    }

    fn jira_settings() -> Settings {
        Settings {
            board: Board::Jira {
                credentials: Some("j".into()),
                project_key: "OPS".into(),
            },
            remote: Remote::Github {
                credentials: None,
                owner: "o".into(),
                repo: "r".into(),
            },
            source: Source::UserFile,
        }
    }

    #[test]
    /// TU-R-039 — opening with a Jira board profile queues a Jira project fetch.
    fn ut_config_queues_jira_fetch() {
        let t = TempDir::new("fetchjira");
        let mut a = app(&t, Some(jira_settings()));
        a.user_config.credentials.insert(
            "j".into(),
            crate::config::Profile::Jira {
                base_url: "https://x".into(),
                email: "e".into(),
                token: "t".into(),
            },
        );
        command(&mut a, "config");
        assert_eq!(
            a.take_fetch_requests(),
            vec![FetchRequest::JiraProjects {
                base_url: "https://x".into(),
                email: "e".into(),
                token: "t".into()
            }]
        );
    }

    #[test]
    /// TU-R-038, TU-R-040 — a GitHub board profile queues a fetch; the dialog waits for it.
    fn ut_config_queues_github_fetch() {
        let t = TempDir::new("fetchgh");
        let mut a = app(&t, Some(settings()));
        a.take_fetch_requests(); // the start-up board request
        command(&mut a, "config");
        assert_eq!(
            a.take_fetch_requests(),
            vec![FetchRequest::GithubProjects {
                token: "t".into(),
                owner: "o".into()
            }]
        );
        assert!(a.take_fetch_requests().is_empty());
        assert!(a.dialog.is_none());
        let rows = render_rows(100, 30, |f| a.render(f));
        assert_eq!(rows[29], "loading projects…");
        key(&mut a, KeyCode::Char('x'));
        let rows = render_rows(100, 30, |f| a.render(f));
        assert_eq!(rows[29], "loading projects…");
    }

    #[test]
    /// TU-E-009 — no stored credentials, no fetch and the dialog opens at once.
    fn ut_no_fetch_without_credentials() {
        let t = TempDir::new("nofetch");
        let mut a = app(&t, None);
        assert!(a.take_fetch_requests().is_empty());
        assert!(a.dialog.is_some());
        let mut repo_file = settings();
        repo_file.board.set_credentials(None);
        repo_file.source = Source::RepoFile;
        let mut a = app(&t, Some(repo_file));
        command(&mut a, "config");
        assert!(a.take_fetch_requests().is_empty());
        assert!(a.dialog.is_some());
    }

    #[test]
    /// TU-R-038, TU-R-041 — a list opens the waiting dialog with a title-only selection; a failure shows the error instead.
    fn ut_message_opens_or_rejects_waiting_dialog() {
        let t = TempDir::new("msg");
        let mut a = app(&t, Some(settings()));
        command(&mut a, "config");
        a.handle_message(Message::GithubProjects(Ok(vec![crate::github::Project {
            number: NonZeroU64::new(1).expect("nz"),
            title: "Roadmap".into(),
        }])));
        let d = a.dialog.as_ref().expect("dialog opened by the list");
        assert!(d.has_selection(Field::BoardProject));
        assert_eq!(d.value(Field::BoardProject), "1");
        let rows = render_rows(100, 30, |f| a.render(f));
        let joined = rows.join("\n");
        assert!(
            joined.contains("Roadmap") && !joined.contains("1 Roadmap"),
            "{joined}"
        );
        assert!(!joined.contains("loading projects"), "{joined}");
        let mut a = app(&t, Some(jira_settings()));
        a.user_config.credentials.insert(
            "j".into(),
            crate::config::Profile::Jira {
                base_url: "https://x".into(),
                email: "e".into(),
                token: "t".into(),
            },
        );
        command(&mut a, "config");
        assert!(a.dialog.is_none(), "waits for the project list");
        a.handle_message(Message::JiraProjects(Err(
            crate::atlassian::AtlassianError::Status(401),
        )));
        assert!(a.dialog.is_none());
        assert_eq!(a.command_line.error(), Some("jira: HTTP 401"));
        key(&mut a, KeyCode::Char('x'));
        assert_eq!(a.command_line.error(), None);
    }

    #[test]
    /// TU-E-015 — a second `config` while waiting replaces the waiting dialog with a new request.
    fn ut_second_config_while_waiting_requeues() {
        let t = TempDir::new("requeue");
        let mut a = app(&t, Some(settings()));
        a.take_fetch_requests(); // the start-up board request
        command(&mut a, "config");
        command(&mut a, "config");
        assert_eq!(a.take_fetch_requests().len(), 2);
        assert!(a.dialog.is_none());
        a.handle_message(Message::GithubProjects(Ok(Vec::new())));
        assert!(a.dialog.is_some());
    }

    #[test]
    /// TU-E-011 — an outcome arriving with no dialog waiting is discarded.
    fn ut_message_without_dialog_is_discarded() {
        let t = TempDir::new("late");
        let mut a = app(&t, Some(settings()));
        a.handle_message(Message::GithubProjects(Ok(Vec::new())));
        assert!(a.dialog.is_none());
        let _ = Choice {
            value: String::new(),
            label: String::new(),
        };
    }

    fn loaded_board() -> crate::github::Board {
        crate::github::Board {
            title: "Roadmap".into(),
            columns: vec![crate::github::board::Column {
                name: "Todo".into(),
                cards: vec![
                    crate::github::Card {
                        id: "I_1".into(),
                        number: 1,
                        title: "First".into(),
                        labels: vec![],
                        assignees: vec![],
                    },
                    crate::github::Card {
                        id: "I_2".into(),
                        number: 2,
                        title: "Second".into(),
                        labels: vec![],
                        assignees: vec![],
                    },
                ],
            }],
        }
    }

    #[test]
    /// TU-R-049, TU-R-050 — a GitHub board with credentials is requested at start and shows loading.
    fn ut_board_requested_at_start() {
        let t = TempDir::new("boardstart");
        let mut a = app(&t, Some(settings()));
        assert_eq!(
            a.take_fetch_requests(),
            vec![FetchRequest::Board {
                token: "t".into(),
                owner: "o".into(),
                number: 1
            }]
        );
        assert!(matches!(a.board, BoardState::Loading));
        let rows = render_rows(60, 6, |f| a.render(f));
        assert!(rows[2].contains("Board is loading.."), "{rows:?}");
    }

    #[test]
    /// TU-E-019 — Jira or missing credentials: no request, summary stays.
    fn ut_board_unavailable_keeps_summary() {
        let t = TempDir::new("boardnone");
        let mut a = app(&t, Some(jira_settings()));
        assert!(a.take_fetch_requests().is_empty());
        assert!(matches!(a.board, BoardState::Unavailable));
        let rows = render_rows(60, 6, |f| a.render(f));
        assert_eq!(&rows[0][3..], "kind: jira");
        let mut without = settings();
        without.board.set_credentials(None);
        let mut a = app(&t, Some(without));
        assert!(a.take_fetch_requests().is_empty());
        assert!(matches!(a.board, BoardState::Unavailable));
    }

    #[test]
    /// TU-R-050, TU-R-051, TU-R-054 — a loaded board renders and takes navigation keys; a failure shows its message.
    fn ut_board_message_loads_or_fails() {
        let t = TempDir::new("boardmsg");
        let mut a = app(&t, Some(settings()));
        a.handle_message(Message::Board(Ok(loaded_board())));
        let rows = render_rows(60, 12, |f| a.render(f));
        assert!(rows[0].contains("Todo (2)"), "{}", rows[0]);
        assert!(rows[3].contains("First"), "{}", rows[3]);
        key(&mut a, KeyCode::Char('j'));
        match &a.board {
            BoardState::Loaded(view) => assert_eq!(view.selected(), Some((0, 1))),
            _ => panic!("board not loaded"),
        }
        a.handle_message(Message::Board(Err(crate::github::GithubError::Status(403))));
        let rows = render_rows(60, 7, |f| a.render(f));
        let row = rows
            .iter()
            .position(|r| r.contains("github: HTTP 403"))
            .expect("error box");
        assert_eq!(row, 2, "vertically centered: {rows:?}");
        let left = rows[row].find('│').expect("border") as i64;
        let width = rows[row].trim_end().chars().count() as i64 - left;
        let body_left = i64::from(tabs::TAB_LINE_WIDTH);
        assert!(
            (left - body_left - (60 - body_left - width) / 2).abs() <= 1,
            "horizontally centered in the body: {}",
            rows[row]
        );
        assert!(rows[1].contains('┌') && rows[3].contains('└'), "{rows:?}");
        command(&mut a, "reload");
        let rows = render_rows(60, 7, |f| a.render(f));
        assert!(
            !rows.iter().any(|r| r.contains("HTTP 403")),
            "hidden on reload: {rows:?}"
        );
    }

    #[test]
    /// TU-R-056 — `:reload` requests the board again, or reports `not configured`.
    fn ut_reload_command() {
        let t = TempDir::new("reload");
        let mut a = app(&t, Some(settings()));
        a.take_fetch_requests();
        a.handle_message(Message::Board(Ok(loaded_board())));
        command(&mut a, "reload");
        assert_eq!(a.take_fetch_requests().len(), 1);
        assert!(matches!(a.board, BoardState::Loading));
        let mut a = app(&t, Some(jira_settings()));
        command(&mut a, "reload");
        assert_eq!(a.command_line.error(), Some("not configured"));
    }

    #[test]
    /// TU-R-049 — a confirmed dialog with GitHub credentials requests the board.
    fn ut_confirm_requests_board() {
        let t = TempDir::new("boardconfirm");
        let mut a = app(&t, None);
        assert!(a.take_fetch_requests().is_empty());
        let d = a.dialog.as_mut().expect("dialog");
        for (f, v) in [
            (Field::Owner, "o"),
            (Field::Repo, "r"),
            (Field::BoardProject, "4"),
            (Field::GithubToken, "tok"),
        ] {
            d.set_value(f, v);
        }
        key(&mut a, KeyCode::Enter);
        assert!(a.dialog.is_none());
        assert_eq!(
            a.take_fetch_requests(),
            vec![
                FetchRequest::Board {
                    token: "tok".into(),
                    owner: "o".into(),
                    number: 4
                },
                FetchRequest::PullRequests {
                    token: "tok".into(),
                    owner: "o".into(),
                    repo: "r".into()
                }
            ]
        );
    }

    #[test]
    /// TU-R-059, TU-R-061, TU-E-020, TU-E-021 — Enter on a card requests its details and opens the overlay, which takes keys until closed; a late result is discarded.
    fn ut_enter_opens_issue_details() {
        let t = TempDir::new("issue");
        let mut a = app(&t, Some(settings()));
        a.take_fetch_requests();
        key(&mut a, KeyCode::Enter);
        assert!(
            a.take_fetch_requests().is_empty() && a.issue.is_none(),
            "no card yet"
        );
        a.handle_message(Message::Board(Ok(loaded_board())));
        key(&mut a, KeyCode::Char('j'));
        key(&mut a, KeyCode::Enter);
        assert_eq!(
            a.take_fetch_requests(),
            vec![FetchRequest::Issue {
                token: "t".into(),
                id: "I_2".into()
            }]
        );
        let rows = render_rows(80, 24, |f| a.render(f));
        assert!(rows.iter().any(|r| r.contains("#2 Second")), "{rows:?}");
        assert!(
            rows.iter().any(|r| r.contains("Loading issue..")),
            "{rows:?}"
        );
        key(&mut a, KeyCode::Char(':'));
        assert!(!a.command_line.is_open(), "overlay takes the key");
        key(&mut a, KeyCode::Esc);
        assert!(a.issue.is_none());
        a.handle_message(Message::Issue(Err(
            crate::github::GithubError::MissingIssue,
        )));
        assert!(a.issue.is_none(), "late result discarded");
        assert!(!a.quit);
    }

    fn remote_settings() -> Settings {
        let mut s = settings();
        s.remote = Remote::Github {
            credentials: Some("gh".into()),
            owner: "o".into(),
            repo: "r".into(),
        };
        s
    }

    #[test]
    /// TU-R-062, TU-R-063, TU-R-064, TU-R-056, TU-E-022 — the pull request list is requested at start and on reload, renders as a table taking `j`/`k`; without credentials the summary stays.
    fn ut_remote_tab_lists_pull_requests() {
        let t = TempDir::new("remote");
        let mut a = app(&t, Some(remote_settings()));
        let requests = a.take_fetch_requests();
        assert!(
            requests.contains(&FetchRequest::PullRequests {
                token: "t".into(),
                owner: "o".into(),
                repo: "r".into()
            }),
            "{requests:?}"
        );
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('1'));
        let rows = render_rows(80, 10, |f| a.render(f));
        assert!(
            rows.iter()
                .any(|r| r.contains("Pull requests are loading..")),
            "{rows:?}"
        );
        let pulls = vec![
            crate::github::pulls::PullRequest {
                number: 5,
                title: "Fix crash".into(),
                ..Default::default()
            },
            crate::github::pulls::PullRequest {
                number: 4,
                title: "Old".into(),
                ..Default::default()
            },
        ];
        a.handle_message(Message::PullRequests(Ok(pulls)));
        let rows = render_rows(80, 10, |f| a.render(f));
        assert!(rows.iter().any(|r| r.contains("Fix crash")), "{rows:?}");
        key(&mut a, KeyCode::Char('j'));
        let RemoteState::Loaded(view) = &a.remote else {
            panic!("loaded");
        };
        assert_eq!(view.selected().map(|p| p.number), Some(4));
        command(&mut a, "reload");
        assert_eq!(a.take_fetch_requests().len(), 2);
        assert!(matches!(a.remote, RemoteState::Loading));
        a.handle_message(Message::PullRequests(Err(
            crate::github::GithubError::MissingRepository,
        )));
        let rows = render_rows(80, 10, |f| a.render(f));
        let row = rows
            .iter()
            .position(|r| r.contains("repository not found"))
            .expect("error box");
        assert!(
            rows[row - 1].contains('┌') && rows[row + 1].contains('└'),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("repository not found")),
            "{rows:?}"
        );

        let mut a = app(&t, Some(settings()));
        assert!(matches!(a.remote, RemoteState::Unavailable));
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('1'));
        let rows = render_rows(80, 10, |f| a.render(f));
        assert!(
            rows.iter().any(|r| r.contains("owner: o")),
            "summary stays: {rows:?}"
        );
    }

    #[test]
    /// TU-R-076, TU-E-039 — a pull request's first changed file is requested at the head commit with the remote token as soon as its details arrive; the arriving content lands in the overlay and asks for nothing more; a selection change in the Files tab requests the next file; content arriving after the overlay closed is discarded.
    fn ut_blob_requests_follow_the_files_tab() {
        use crate::github::blob::Blob;
        use crate::github::files::{ChangedFile, FileStatus};
        let t = TempDir::new("blob");
        let mut a = app(&t, Some(remote_settings()));
        a.take_fetch_requests();
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('1'));
        a.handle_message(Message::PullRequests(Ok(vec![
            crate::github::pulls::PullRequest {
                number: 5,
                ..Default::default()
            },
        ])));
        key(&mut a, KeyCode::Enter);
        a.take_fetch_requests();
        let file = |path: &str| ChangedFile {
            path: path.into(),
            previous_path: None,
            status: FileStatus::Modified,
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1 +1 @@\n-x\n+y\n".into()),
        };
        a.handle_message(Message::PullRequest(Ok(crate::github::pull::PullDetails {
            number: 5,
            head_oid: "0123abcd".into(),
            repository: "o/r".into(),
            files: vec![file("b.rs"), file("a.rs")],
            ..Default::default()
        })));
        let blob = |path: &str| FetchRequest::Blob {
            token: "t".into(),
            owner: "o".into(),
            repo: "r".into(),
            oid: "0123abcd".into(),
            path: path.into(),
        };
        assert_eq!(a.take_fetch_requests(), vec![blob("a.rs")]);
        a.handle_message(Message::Blob {
            path: "a.rs".into(),
            result: Ok(Blob::Text("y\n".into())),
        });
        assert!(a.take_fetch_requests().is_empty());
        let rows = render_rows(100, 30, |f| a.render(f));
        assert!(
            !rows.iter().any(|r| r.contains("Loading file..")),
            "{rows:?}"
        );
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('2'));
        let rows = render_rows(100, 30, |f| a.render(f));
        assert!(
            rows.iter()
                .any(|r| r.contains("1 -x") && r.contains("1 +y")),
            "{rows:?}"
        );
        key(&mut a, KeyCode::Char('j'));
        assert_eq!(a.take_fetch_requests(), vec![blob("b.rs")]);
        key(&mut a, KeyCode::Esc);
        assert!(a.pull.is_none());
        a.handle_message(Message::Blob {
            path: "b.rs".into(),
            result: Ok(Blob::Binary),
        });
        assert!(a.take_fetch_requests().is_empty());
    }

    #[test]
    /// TU-R-071, TU-E-030 — Enter on a Development entry closes the overlay, switches the tab and opens the linked item's overlay, whatever the target list's state.
    fn ut_development_enter_switches_tabs() {
        let t = TempDir::new("dev");
        let mut a = app(&t, Some(remote_settings()));
        a.take_fetch_requests();
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('1'));
        a.handle_message(Message::PullRequests(Ok(vec![
            crate::github::pulls::PullRequest {
                number: 5,
                ..Default::default()
            },
        ])));
        key(&mut a, KeyCode::Enter);
        a.take_fetch_requests();
        a.handle_message(Message::PullRequest(Ok(crate::github::pull::PullDetails {
            number: 5,
            development: vec![crate::github::pull::IssueRef {
                id: "I_7".into(),
                number: 7,
                title: "Crash".into(),
            }],
            ..Default::default()
        })));
        for _ in 0..5 {
            key(&mut a, KeyCode::Tab);
        }
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.active_tab, Tab::Board);
        assert!(a.pull.is_none() && a.issue.is_some());
        assert_eq!(
            a.take_fetch_requests(),
            vec![FetchRequest::Issue {
                token: "t".into(),
                id: "I_7".into()
            }]
        );
        let rows = render_rows(80, 24, |f| a.render(f));
        assert!(rows.iter().any(|r| r.contains("#7 Crash")), "{rows:?}");
        a.handle_message(Message::Issue(Ok(crate::github::issue::Issue {
            number: 7,
            development: vec![crate::github::issue::PullRef {
                number: 9,
                title: "Fix".into(),
                repository: "x/y".into(),
            }],
            ..Default::default()
        })));
        for _ in 0..5 {
            key(&mut a, KeyCode::Tab);
        }
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.active_tab, Tab::Remote);
        assert!(a.issue.is_none() && a.pull.is_some());
        assert_eq!(
            a.take_fetch_requests(),
            vec![FetchRequest::PullRequest {
                token: "t".into(),
                owner: "x".into(),
                repo: "y".into(),
                number: 9
            }]
        );
        let rows = render_rows(80, 24, |f| a.render(f));
        assert!(rows.iter().any(|r| r.contains("#9 Fix")), "{rows:?}");
    }

    #[test]
    /// TU-R-065, TU-R-067, TU-E-025 — Enter on a row requests the pull request and opens the overlay, which takes keys until closed; a late result is discarded.
    fn ut_enter_opens_pull_details() {
        let t = TempDir::new("pull");
        let mut a = app(&t, Some(remote_settings()));
        a.take_fetch_requests();
        a.handle_key(KeyModifiers::CONTROL, KeyCode::Char('t'));
        key(&mut a, KeyCode::Char('1'));
        key(&mut a, KeyCode::Enter);
        assert!(
            a.take_fetch_requests().is_empty() && a.pull.is_none(),
            "no row yet"
        );
        a.handle_message(Message::PullRequests(Ok(vec![
            crate::github::pulls::PullRequest {
                number: 5,
                title: "Fix crash".into(),
                ..Default::default()
            },
        ])));
        key(&mut a, KeyCode::Enter);
        assert_eq!(
            a.take_fetch_requests(),
            vec![FetchRequest::PullRequest {
                token: "t".into(),
                owner: "o".into(),
                repo: "r".into(),
                number: 5
            }]
        );
        let rows = render_rows(80, 24, |f| a.render(f));
        assert!(rows.iter().any(|r| r.contains("#5 Fix crash")), "{rows:?}");
        assert!(
            rows.iter().any(|r| r.contains("Loading pull request..")),
            "{rows:?}"
        );
        key(&mut a, KeyCode::Char(':'));
        assert!(!a.command_line.is_open(), "overlay takes the key");
        key(&mut a, KeyCode::Esc);
        assert!(a.pull.is_none());
        a.handle_message(Message::PullRequest(Err(
            crate::github::GithubError::MissingPullRequest,
        )));
        assert!(a.pull.is_none(), "late result discarded");
    }
}
