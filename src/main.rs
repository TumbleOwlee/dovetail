//! prodgy: one terminal for the task board, the git remote, and the spec-driven workflow.

mod app;
mod atlassian;
mod command;
mod config;
mod event;
mod github;
#[cfg(test)]
mod testkit;
mod view;

use std::io::Stdout;

use clap::Parser;
use ferrowl_ui::AlternateScreen;

use crate::app::App;

#[derive(Parser)]
#[command(version, about)]
struct Cli {}

#[tokio::main]
async fn main() {
    let Cli {} = Cli::parse();
    let mut app = match App::init() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    // Release the terminal on panic so the message is readable.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        AlternateScreen::<Stdout>::release();
        hook(panic);
    }));

    let mut screen = match AlternateScreen::<Stdout>::new() {
        Ok(screen) => screen,
        Err(e) => {
            eprintln!("error: failed to set up the terminal: {e}");
            std::process::exit(1);
        }
    };
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let (message_tx, message_rx) = tokio::sync::mpsc::channel::<event::Message>(64);
    event::spawn_terminal_reader(tx);
    let outcome = event::run(&mut app, &mut screen, rx, message_tx, message_rx).await;
    drop(screen);
    if let Err(e) = outcome {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
