//! The `:` command line's vocabulary: parsing typed text into a [`Cmd`] and the help rows
//! advertising it.

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
    Reload,
    Unknown(String),
}

/// Usage and description of every command, in the order the help box lists them.
pub const HELP: [(&str, &str); 7] = [
    (":q", "quit"),
    (":config", "open the configuration dialog"),
    (":board", "show the Task Board tab"),
    (":remote", "show the Git Remote tab"),
    (":w", "write the settings to the user-level config"),
    (":wr", "write .prodgy.toml (credentials stripped)"),
    (":reload", "load the board again"),
];

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
        "reload" => Cmd::Reload,
        other => Cmd::Unknown(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// TU-R-029, TU-R-030, TU-R-031, TU-R-032, TU-R-033, TU-R-056 — each name parses to its command.
    fn ut_parse_known_commands() {
        assert_eq!(parse("q"), Cmd::Quit);
        assert_eq!(parse("config"), Cmd::Config);
        assert_eq!(parse("board"), Cmd::Board);
        assert_eq!(parse("remote"), Cmd::Remote);
        assert_eq!(parse("w"), Cmd::Write);
        assert_eq!(parse("wr"), Cmd::WriteRepo);
        assert_eq!(parse("reload"), Cmd::Reload);
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
    /// TU-R-026 — every command has a help row, and every help row parses to a command.
    fn ut_help_rows_cover_every_command() {
        let parsed: Vec<Cmd> = HELP
            .iter()
            .map(|(usage, _)| {
                parse(
                    usage
                        .trim_start_matches(':')
                        .split(' ')
                        .next()
                        .unwrap_or(""),
                )
            })
            .collect();
        assert_eq!(
            parsed,
            vec![
                Cmd::Quit,
                Cmd::Config,
                Cmd::Board,
                Cmd::Remote,
                Cmd::Write,
                Cmd::WriteRepo,
                Cmd::Reload
            ]
        );
        assert!(
            HELP.iter()
                .all(|(usage, desc)| usage.starts_with(':') && !desc.is_empty())
        );
    }
}
