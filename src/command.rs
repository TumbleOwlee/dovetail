//! The `:` command line's vocabulary: parsing typed text into a [`Cmd`] and completing
//! command names.

use ferrowl_ui::traits::{Suggestion, SuggestionProvider};

/// A parsed command-line entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    /// Nothing typed: closes the command line and does nothing.
    Empty,
    Quit,
    Config,
    Board,
    Remote,
    Write,
    WriteRepo,
    Unknown(String),
}

/// Every command name, in the order suggestions list them.
pub const NAMES: [&str; 6] = ["board", "config", "q", "remote", "w", "wr"];

/// Parses trimmed input into a command; unknown text keeps its trimmed form.
pub fn parse(input: &str) -> Cmd {
    match input.trim() {
        "" => Cmd::Empty,
        "q" => Cmd::Quit,
        "config" => Cmd::Config,
        "board" => Cmd::Board,
        "remote" => Cmd::Remote,
        "w" => Cmd::Write,
        "wr" => Cmd::WriteRepo,
        other => Cmd::Unknown(other.to_string()),
    }
}

/// Completes command names by prefix.
#[derive(Debug, Clone, Default)]
pub struct CommandProvider;

impl SuggestionProvider for CommandProvider {
    fn suggest(&self, input: &str) -> Vec<Suggestion> {
        if input.is_empty() {
            return Vec::new();
        }
        NAMES
            .iter()
            .filter(|name| name.starts_with(input))
            .map(|name| Suggestion {
                value: name.to_string(),
                label: name.to_string(),
                partial: false,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// TU-R-029, TU-R-030, TU-R-031, TU-R-032, TU-R-033 — each name parses to its command.
    fn ut_parse_known_commands() {
        assert_eq!(parse("q"), Cmd::Quit);
        assert_eq!(parse("config"), Cmd::Config);
        assert_eq!(parse("board"), Cmd::Board);
        assert_eq!(parse("remote"), Cmd::Remote);
        assert_eq!(parse("w"), Cmd::Write);
        assert_eq!(parse("wr"), Cmd::WriteRepo);
    }

    #[test]
    /// TU-E-004, TU-E-005 — whitespace is trimmed; nothing left is `Empty`.
    fn ut_parse_trims_and_empty() {
        assert_eq!(parse(" q "), Cmd::Quit);
        assert_eq!(parse(""), Cmd::Empty);
        assert_eq!(parse("   "), Cmd::Empty);
    }

    #[test]
    /// TU-R-034 — unrecognised text is kept, trimmed, for the error message.
    fn ut_parse_unknown_keeps_text() {
        assert_eq!(parse(" frob 1 "), Cmd::Unknown("frob 1".into()));
    }

    #[test]
    /// TU-R-026 — suggestions are the names starting with the input, none for an empty input.
    fn ut_suggest_by_prefix() {
        let p = CommandProvider;
        let values = |s: &str| {
            p.suggest(s)
                .into_iter()
                .map(|s| s.value)
                .collect::<Vec<_>>()
        };
        assert_eq!(values("w"), vec!["w", "wr"]);
        assert_eq!(values("co"), vec!["config"]);
        assert_eq!(values("z"), Vec::<String>::new());
        assert_eq!(values(""), Vec::<String>::new());
        assert!(
            p.suggest("b")
                .iter()
                .all(|s| !s.partial && s.label == s.value)
        );
    }
}
