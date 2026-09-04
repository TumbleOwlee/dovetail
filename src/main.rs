//! prodgy: one terminal for the task board, the git remote, and the spec-driven workflow.

mod app;
mod command;
mod config;
mod event;
#[cfg(test)]
mod testkit;
mod view;

use std::io::Stdout;
use std::path::PathBuf;

use clap::Parser;
use ferrowl_ui::AlternateScreen;

use crate::app::App;
use crate::config::{ConfigError, Origin, paths, store};

#[derive(Parser)]
#[command(version, about)]
struct Cli {}

/// Everything that runs before the alternate screen: resolve the repository, load both files.
fn prepare() -> Result<App, ConfigError> {
    let cwd = std::env::current_dir().map_err(|source| ConfigError::Io {
        path: PathBuf::from("."),
        source,
    })?;
    let root = paths::find_repo_root(&cwd).ok_or_else(|| ConfigError::NoRepository(cwd.clone()))?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let user_path = paths::user_config_path(std::env::var_os("XDG_CONFIG_HOME").as_deref(), &home);
    let user_config = store::load_user_config(&user_path)?;
    let repo_file = store::load_repo_config(&paths::repo_config_path(&root))?;
    let settings = store::resolve(&user_config, repo_file, &root);
    let origin = Origin::of_repo(&root);
    Ok(App::new(root, user_path, user_config, settings, origin))
}

#[tokio::main]
async fn main() {
    let Cli {} = Cli::parse();
    let mut app = match prepare() {
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
    let (_message_tx, message_rx) = tokio::sync::mpsc::channel::<event::Message>(64);
    event::spawn_terminal_reader(tx);
    let outcome = event::run(&mut app, &mut screen, rx, message_rx).await;
    drop(screen);
    if let Err(e) = outcome {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
