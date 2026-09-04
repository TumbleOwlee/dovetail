//! Application state: what is held, which layer has the keyboard, and the frame layout.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::command::{self, Cmd};
use crate::config::profile::{profile_base, store_profile};
use crate::config::{
    Board, ConfigError, Origin, Profile, Section, Settings, Source, UserConfig, paths, store,
};
use crate::event::Message;
use crate::view::command_line::{CommandLine, CommandLineEvent};
use crate::view::dialog::config::{BoardForm, ConfigDialog, DialogEvent, RemoteForm};
use crate::view::dialog::config::{Choice, Field};
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
}

pub struct App {
    pub repo_root: PathBuf,
    pub user_path: PathBuf,
    pub user_config: UserConfig,
    pub settings: Option<Settings>,
    pub origin: Option<Origin>,
    pub active_tab: Tab,
    pub dialog: Option<ConfigDialog>,
    pub command_line: CommandLine,
    pending_fetches: Vec<FetchRequest>,
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
        App {
            repo_root,
            user_path,
            user_config,
            settings,
            origin,
            active_tab: Tab::Board,
            dialog,
            command_line: CommandLine::new(),
            pending_fetches: Vec::new(),
            tab_prefix: false,
            quit: false,
        }
    }

    /// Fetches queued since the last call, for the loop to run.
    pub fn take_fetch_requests(&mut self) -> Vec<FetchRequest> {
        std::mem::take(&mut self.pending_fetches)
    }

    /// A fetch outcome; ignored when no dialog is open.
    pub fn handle_message(&mut self, message: Message) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        match message {
            Message::GithubProjects(Ok(projects)) => dialog.set_options(
                Field::BoardProject,
                projects
                    .into_iter()
                    .map(|p| Choice {
                        value: p.number.to_string(),
                        label: format!("{} {}", p.number, p.title),
                    })
                    .collect(),
            ),
            Message::GithubProjects(Err(e)) => {
                dialog.set_unavailable(Field::BoardProject, e.to_string());
            }
            Message::JiraProjects(Ok(projects)) => dialog.set_options(
                Field::JiraProjectKey,
                projects
                    .into_iter()
                    .map(|p| Choice {
                        label: format!("{} {}", p.key, p.name),
                        value: p.key,
                    })
                    .collect(),
            ),
            Message::JiraProjects(Err(e)) => {
                dialog.set_unavailable(Field::JiraProjectKey, e.to_string());
            }
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
            _ => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let [top, middle, bottom] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let present = self
            .settings
            .as_ref()
            .is_some_and(|s| self.credentials_present(self.active_tab.section(s)));
        let lines = tabs::summary_lines(self.active_tab, self.settings.as_ref(), present);
        let buf = frame.buffer_mut();
        tabs::render_tab_line(top, buf, self.active_tab, self.settings.as_ref());
        tabs::render_body(middle, buf, &lines);
        self.command_line.render(bottom, buf);
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

    fn open_dialog(&mut self) {
        let Some(settings) = &self.settings else {
            self.dialog = Some(ConfigDialog::new(self.origin.as_ref()));
            return;
        };
        let mut dialog =
            ConfigDialog::from_settings(settings, &self.user_config, self.origin.as_ref());
        let profile = settings
            .board
            .credentials()
            .and_then(|name| self.user_config.credentials.get(name));
        match (&settings.board, profile) {
            (Board::Github { owner, .. }, Some(Profile::Github { token })) => {
                self.pending_fetches.push(FetchRequest::GithubProjects {
                    token: token.clone(),
                    owner: owner.clone(),
                });
                dialog.set_loading(Field::BoardProject);
            }
            (
                Board::Jira { .. },
                Some(Profile::Jira {
                    base_url,
                    email,
                    token,
                }),
            ) => {
                self.pending_fetches.push(FetchRequest::JiraProjects {
                    base_url: base_url.clone(),
                    email: email.clone(),
                    token: token.clone(),
                });
                dialog.set_loading(Field::JiraProjectKey);
            }
            _ => {}
        }
        self.dialog = Some(dialog);
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
        // A prefix match opens completion; Esc closes only the popup, the line stays open.
        if a.command_line.suggestions_open() {
            key(a, KeyCode::Esc);
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
        let rows = render_rows(60, 10, |f| a.render(f));
        assert!(rows[0].contains("[0] Task Board [GitHub]"), "{}", rows[0]);
        assert_eq!(rows[1], "kind: github");
        assert_eq!(rows[9], "unknown command: frob");
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
        assert!(joined.contains("[0] Task Board [-]"), "{joined}");
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
    /// TU-R-038 — opening with a GitHub board profile queues a project fetch and marks the field.
    fn ut_config_queues_github_fetch() {
        let t = TempDir::new("fetchgh");
        let mut a = app(&t, Some(settings()));
        assert!(a.take_fetch_requests().is_empty());
        command(&mut a, "config");
        assert_eq!(
            a.take_fetch_requests(),
            vec![FetchRequest::GithubProjects {
                token: "t".into(),
                owner: "o".into()
            }]
        );
        assert!(a.take_fetch_requests().is_empty());
        let rows = render_rows(100, 30, |f| a.render(f));
        assert!(rows.join("\n").contains("(loading…)"));
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
    /// TU-E-009 — no stored credentials, no fetch: first run and repository-file settings alike.
    fn ut_no_fetch_without_credentials() {
        let t = TempDir::new("nofetch");
        let mut a = app(&t, None);
        assert!(a.take_fetch_requests().is_empty());
        let mut repo_file = settings();
        repo_file.board.set_credentials(None);
        repo_file.source = Source::RepoFile;
        let mut a = app(&t, Some(repo_file));
        command(&mut a, "config");
        assert!(a.take_fetch_requests().is_empty());
    }

    #[test]
    /// TU-R-038, TU-R-041 — outcomes reach the open dialog as options or as an error.
    fn ut_message_updates_open_dialog() {
        let t = TempDir::new("msg");
        let mut a = app(&t, Some(settings()));
        command(&mut a, "config");
        a.handle_message(Message::GithubProjects(Ok(vec![crate::github::Project {
            number: NonZeroU64::new(1).expect("nz"),
            title: "Roadmap".into(),
        }])));
        let d = a.dialog.as_ref().expect("dialog");
        assert!(d.has_selection(Field::BoardProject));
        assert_eq!(d.value(Field::BoardProject), "1");
        let mut a = app(&t, Some(jira_settings()));
        command(&mut a, "config");
        a.handle_message(Message::JiraProjects(Err(
            crate::atlassian::AtlassianError::Status(401),
        )));
        let d = a.dialog.as_ref().expect("dialog");
        assert!(!d.has_selection(Field::JiraProjectKey));
        assert_eq!(d.error(), Some("jira: HTTP 401"));
    }

    #[test]
    /// TU-E-011 — an outcome arriving with no dialog open is discarded.
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
}
