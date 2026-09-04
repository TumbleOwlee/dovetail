//! The async loop: draw, then wait for a terminal event or an application message.

use crossterm::event::{Event, KeyEventKind};
use ferrowl_ui::DrawSurface;
use tokio::sync::mpsc;

use crate::app::{App, FetchRequest};
use crate::atlassian::{self, AtlassianError, JiraProject};
use crate::github::{self, GithubError, Project};

/// Results other tasks send to the loop.
#[derive(Debug)]
pub enum Message {
    GithubProjects(Result<Vec<Project>, GithubError>),
    JiraProjects(Result<Vec<JiraProject>, AtlassianError>),
}

/// Performs one fetch and sends its outcome; a dropped receiver ends it silently.
pub async fn dispatch(request: FetchRequest, client: reqwest::Client, tx: mpsc::Sender<Message>) {
    let message = match request {
        FetchRequest::GithubProjects { token, owner } => {
            Message::GithubProjects(github::projects::list_projects(&client, &token, &owner).await)
        }
        FetchRequest::JiraProjects {
            base_url,
            email,
            token,
        } => Message::JiraProjects(
            atlassian::projects::list_projects(&client, &base_url, &email, &token).await,
        ),
    };
    let _ = tx.send(message).await;
}

/// Reads terminal events on a blocking thread into `tx` until the receiver is dropped.
pub fn spawn_terminal_reader(tx: mpsc::Sender<Event>) {
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if tx.blocking_send(event).is_err() {
                break;
            }
        }
    });
}

/// Runs until the app asks to quit or both channels close.
pub async fn run<S: DrawSurface>(
    app: &mut App,
    screen: &mut S,
    mut events: mpsc::Receiver<Event>,
    message_tx: mpsc::Sender<Message>,
    mut messages: mpsc::Receiver<Message>,
) -> std::io::Result<()> {
    let client = reqwest::Client::new();
    loop {
        for request in app.take_fetch_requests() {
            tokio::spawn(dispatch(request, client.clone(), message_tx.clone()));
        }
        screen.draw(|frame| app.render(frame))?;
        tokio::select! {
            event = events.recv() => match event {
                Some(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                    app.handle_key(key.modifiers, key.code);
                }
                Some(_) => {}
                None => break,
            },
            message = messages.recv() => match message {
                Some(message) => app.handle_message(message),
                None => break,
            },
        }
        if app.should_quit() {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::UserConfig;
    use crate::testkit::TempDir;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    struct TestSurface(Terminal<TestBackend>);

    impl DrawSurface for TestSurface {
        fn draw<F: FnOnce(&mut ratatui::Frame)>(&mut self, render: F) -> std::io::Result<()> {
            self.0
                .draw(render)
                .map(|_| ())
                .map_err(|never| match never {})
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[tokio::test]
    /// TU-R-029, TU-R-004 — keys from the channel reach the app and `:q` ends the loop.
    async fn ut_loop_quits_on_q_command() {
        let t = TempDir::new("loop");
        let root = t.path().join("repo");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        let mut app = App::new(
            root,
            t.path().join("cfg.toml"),
            UserConfig::default(),
            None,
            None,
        );
        let mut screen = TestSurface(Terminal::new(TestBackend::new(80, 24)).expect("backend"));
        let (tx, rx) = mpsc::channel(8);
        let (mtx, mrx) = mpsc::channel::<Message>(1);
        // The first-run dialog is open: Esc quits it (TU-R-016).
        tx.send(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )))
        .await
        .expect("send");
        tx.send(key(KeyCode::Esc)).await.expect("send");
        run(&mut app, &mut screen, rx, mtx, mrx)
            .await
            .expect("loop");
        assert!(app.should_quit());
    }

    #[tokio::test]
    /// AT-R-002, TU-R-041 — a fetch outcome travels back as a message, errors included.
    async fn ut_dispatch_sends_outcome() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let (tx, mut rx) = mpsc::channel(1);
        dispatch(
            FetchRequest::JiraProjects {
                base_url: format!("http://127.0.0.1:{port}"),
                email: "e".into(),
                token: "t".into(),
            },
            reqwest::Client::new(),
            tx,
        )
        .await;
        assert!(matches!(
            rx.recv().await,
            Some(Message::JiraProjects(Err(AtlassianError::Http(_))))
        ));
    }

    #[tokio::test]
    /// TU-R-004 — a closed event channel ends the loop without quitting the app.
    async fn ut_loop_ends_when_channel_closes() {
        let t = TempDir::new("loopclose");
        let root = t.path().join("repo");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        let mut app = App::new(
            root,
            t.path().join("cfg.toml"),
            UserConfig::default(),
            None,
            None,
        );
        let mut screen = TestSurface(Terminal::new(TestBackend::new(80, 24)).expect("backend"));
        let (tx, rx) = mpsc::channel(8);
        let (mtx, mrx) = mpsc::channel::<Message>(1);
        tx.send(Event::Resize(10, 10)).await.expect("send");
        drop(tx);
        run(&mut app, &mut screen, rx, mtx, mrx)
            .await
            .expect("loop");
        assert!(!app.should_quit());
    }
}
