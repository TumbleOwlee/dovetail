//! The async loop: draw, then wait for a terminal event or an application message.

use crossterm::event::{Event, KeyEventKind};
use ferrowl_ui::DrawSurface;
use tokio::sync::mpsc;

use crate::app::App;

/// Messages other tasks send to the loop. Empty until an integration needs one.
#[derive(Debug)]
pub enum Message {}

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
    mut messages: mpsc::Receiver<Message>,
) -> std::io::Result<()> {
    loop {
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
                Some(message) => match message {},
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
        let (_mtx, mrx) = mpsc::channel::<Message>(1);
        // The first-run dialog is open: Esc quits it (TU-R-016).
        tx.send(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )))
        .await
        .expect("send");
        tx.send(key(KeyCode::Esc)).await.expect("send");
        run(&mut app, &mut screen, rx, mrx).await.expect("loop");
        assert!(app.should_quit());
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
        let (_mtx, mrx) = mpsc::channel::<Message>(1);
        tx.send(Event::Resize(10, 10)).await.expect("send");
        drop(tx);
        run(&mut app, &mut screen, rx, mrx).await.expect("loop");
        assert!(!app.should_quit());
    }
}
