//! Parsing, validation, resolution and writing of the two files.

use std::path::{Path, PathBuf};

use super::error::ConfigError;
use super::schema::{Board, Remote, RepoConfig, RepoEntry, Section, UserConfig};

/// Which file the held settings came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    RepoFile,
    UserFile,
}

/// The active repository's resolved sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub board: Board,
    pub remote: Remote,
    pub source: Source,
}

/// Parses and validates user-level file text. `path` only labels errors.
pub fn parse_user_config(text: &str, path: &Path) -> Result<UserConfig, ConfigError> {
    let user: UserConfig = toml::from_str(text).map_err(|e| parse_error(path, &e))?;
    let mut seen: Vec<PathBuf> = Vec::new();
    for entry in &user.repo {
        let key = canonical(&entry.path);
        if seen.contains(&key) {
            return Err(ConfigError::DuplicateRepoPath {
                path: path.to_path_buf(),
                repo: entry.path.clone(),
            });
        }
        seen.push(key);
        check_reference(&user, &entry.board, path)?;
        check_reference(&user, &entry.remote, path)?;
    }
    Ok(user)
}

fn check_reference(
    user: &UserConfig,
    section: &dyn Section,
    path: &Path,
) -> Result<(), ConfigError> {
    let Some(reference) = section.credentials() else {
        return Ok(());
    };
    let profile = user
        .credentials
        .get(reference)
        .ok_or_else(|| ConfigError::UnknownProfile {
            path: path.to_path_buf(),
            reference: reference.to_string(),
        })?;
    if profile.kind() != section.kind() {
        return Err(ConfigError::ProfileKindMismatch {
            path: path.to_path_buf(),
            reference: reference.to_string(),
            section_kind: section.kind(),
            profile_kind: profile.kind(),
        });
    }
    Ok(())
}

fn parse_error(path: &Path, error: &toml::de::Error) -> ConfigError {
    // toml's Display spans several lines with a source excerpt; the message alone is one line.
    ConfigError::Parse {
        path: path.to_path_buf(),
        message: error.message().to_string(),
    }
}

/// Parses and validates repository-level file text. `path` only labels errors.
pub fn parse_repo_config(text: &str, path: &Path) -> Result<RepoConfig, ConfigError> {
    let table: toml::Table = toml::from_str(text).map_err(|e| parse_error(path, &e))?;
    let section_has_credentials = |name: &str| {
        table
            .get(name)
            .and_then(toml::Value::as_table)
            .is_some_and(|t| t.contains_key("credentials"))
    };
    if table.contains_key("credentials")
        || section_has_credentials("board")
        || section_has_credentials("remote")
    {
        return Err(ConfigError::RepoFileCredentials {
            path: path.to_path_buf(),
        });
    }
    table.try_into().map_err(|e| parse_error(path, &e))
}

/// Reads the user-level file; a missing file is an empty configuration.
pub fn load_user_config(path: &Path) -> Result<UserConfig, ConfigError> {
    match read(path)? {
        Some(text) => parse_user_config(&text, path),
        None => Ok(UserConfig::default()),
    }
}

fn read(path: &Path) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Reads the repository-level file; `None` when it does not exist.
pub fn load_repo_config(path: &Path) -> Result<Option<RepoConfig>, ConfigError> {
    read(path)?
        .map(|text| parse_repo_config(&text, path))
        .transpose()
}

/// The repository file wins; otherwise the user entry whose canonical path matches `root`.
pub fn resolve(user: &UserConfig, repo_file: Option<RepoConfig>, root: &Path) -> Option<Settings> {
    if let Some(repo) = repo_file {
        return Some(Settings {
            board: repo.board,
            remote: repo.remote,
            source: Source::RepoFile,
        });
    }
    entry_for(user, root).map(|entry| Settings {
        board: entry.board.clone(),
        remote: entry.remote.clone(),
        source: Source::UserFile,
    })
}

/// Replaces the entry for `root` in place, or appends one.
pub fn upsert_repo(user: &mut UserConfig, root: &Path, board: Board, remote: Remote) {
    let key = canonical(root);
    match user.repo.iter_mut().find(|e| canonical(&e.path) == key) {
        Some(entry) => {
            entry.board = board;
            entry.remote = remote;
        }
        None => user.repo.push(RepoEntry {
            path: root.to_path_buf(),
            board,
            remote,
        }),
    }
}

/// Serializes `user` to `path`, creating parent directories.
pub fn save_user_config(path: &Path, user: &UserConfig) -> Result<(), ConfigError> {
    let text = toml::to_string_pretty(user).map_err(|e| ConfigError::Serialize {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    write(path, &text)
}

fn write(path: &Path, text: &str) -> Result<(), ConfigError> {
    let io = |source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(path, text).map_err(io)
}

/// Serializes the sections without `credentials` to `path`, creating parent directories.
pub fn save_repo_config(path: &Path, board: &Board, remote: &Remote) -> Result<(), ConfigError> {
    let mut board = board.clone();
    let mut remote = remote.clone();
    board.set_credentials(None);
    remote.set_credentials(None);
    let text = toml::to_string_pretty(&RepoConfig { board, remote }).map_err(|e| {
        ConfigError::Serialize {
            path: path.to_path_buf(),
            message: e.to_string(),
        }
    })?;
    write(path, &text)
}

/// The user entry for `root`, if any.
pub fn entry_for<'a>(user: &'a UserConfig, root: &Path) -> Option<&'a RepoEntry> {
    let key = canonical(root);
    user.repo.iter().find(|e| canonical(&e.path) == key)
}

/// Canonical form when the path exists, the path itself otherwise, so entries for absent
/// directories still compare by their literal text.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{Kind, Profile, Section};
    use crate::testkit::TempDir;
    use std::num::NonZeroU64;

    const USER: &str = r#"
[credentials.gh]
kind = "github"
token = "t"

[[repo]]
path = "/first"
[repo.board]
kind = "github"
credentials = "gh"
owner = "o"
repo = "r"
project = 1
[repo.remote]
kind = "github"
owner = "o"
repo = "r"
"#;

    fn board(project: u64) -> Board {
        Board::Github {
            credentials: None,
            owner: "o".into(),
            repo: "r".into(),
            project: NonZeroU64::new(project).expect("non-zero"),
        }
    }

    fn remote() -> Remote {
        Remote::Bitbucket {
            credentials: None,
            workspace: "w".into(),
            repo: "s".into(),
        }
    }

    fn label() -> PathBuf {
        PathBuf::from("/cfg.toml")
    }

    #[test]
    /// CF-R-023 — duplicate `[[repo]]` paths are a load error naming the path.
    fn ut_duplicate_repo_path_is_error() {
        let text = format!(
            "{USER}\n[[repo]]\npath = \"/first\"\n[repo.board]\nkind = \"jira\"\nproject_key = \"A\"\n[repo.remote]\nkind = \"github\"\nowner = \"o\"\nrepo = \"r\"\n"
        );
        let err = parse_user_config(&text, &label()).expect_err("rejects");
        assert!(
            matches!(err, ConfigError::DuplicateRepoPath { .. }),
            "{err}"
        );
        assert!(err.to_string().contains("/first"));
    }

    #[test]
    /// CF-R-024 — a reference to a missing profile is a load error naming the reference.
    fn ut_unknown_profile_reference_is_error() {
        let text = USER.replace("credentials = \"gh\"", "credentials = \"nope\"");
        let err = parse_user_config(&text, &label()).expect_err("rejects");
        assert!(matches!(err, ConfigError::UnknownProfile { .. }), "{err}");
        assert!(err.to_string().contains("nope"));
    }

    #[test]
    /// CF-R-025 — a profile of the wrong kind is a load error naming both kinds.
    fn ut_profile_kind_mismatch_is_error() {
        let text = USER.replace(
            "[repo.remote]\nkind = \"github\"\nowner = \"o\"\nrepo = \"r\"",
            "[repo.remote]\nkind = \"bitbucket\"\ncredentials = \"gh\"\nworkspace = \"w\"\nrepo = \"s\"",
        );
        let err = parse_user_config(&text, &label()).expect_err("rejects");
        match &err {
            ConfigError::ProfileKindMismatch {
                section_kind,
                profile_kind,
                ..
            } => {
                assert_eq!(*section_kind, Kind::Bitbucket);
                assert_eq!(*profile_kind, Kind::Github);
            }
            other => panic!("unexpected {other}"),
        }
        assert!(err.to_string().contains("bitbucket") && err.to_string().contains("github"));
    }

    #[test]
    /// CF-R-022, CF-R-027 — a syntax or schema error is a single-line parse error.
    fn ut_parse_error_is_single_line() {
        let err = parse_user_config("this is not toml", &label()).expect_err("rejects");
        assert!(matches!(err, ConfigError::Parse { .. }), "{err}");
        assert!(!err.to_string().contains('\n'), "{err:?}");
    }

    #[test]
    /// CF-R-026 — `credentials` anywhere in the repository-level file is a load error.
    fn ut_repo_file_credentials_rejected() {
        let section = "[board]\nkind = \"jira\"\ncredentials = \"x\"\nproject_key = \"A\"\n[remote]\nkind = \"github\"\nowner = \"o\"\nrepo = \"r\"\n";
        let err = parse_repo_config(section, &label()).expect_err("rejects");
        assert!(
            matches!(err, ConfigError::RepoFileCredentials { .. }),
            "{err}"
        );
        let table = "[credentials.x]\nkind = \"github\"\ntoken = \"t\"\n[board]\nkind = \"jira\"\nproject_key = \"A\"\n[remote]\nkind = \"github\"\nowner = \"o\"\nrepo = \"r\"\n";
        let err = parse_repo_config(table, &label()).expect_err("rejects");
        assert!(
            matches!(err, ConfigError::RepoFileCredentials { .. }),
            "{err}"
        );
    }

    #[test]
    /// CF-R-008 — a missing user-level file loads as the empty configuration.
    fn ut_missing_user_file_is_empty() {
        let t = TempDir::new("missing");
        let cfg = load_user_config(&t.path().join("none.toml")).expect("ok");
        assert_eq!(cfg, UserConfig::default());
    }

    #[test]
    /// CF-R-005, CF-R-009 — the repository file wins and carries no profile reference.
    fn ut_resolve_prefers_repo_file() {
        let t = TempDir::new("resolve");
        let root = t.path().join("first");
        std::fs::create_dir_all(&root).expect("mkdir");
        let user = parse_user_config(
            &USER.replace("/first", &root.display().to_string()),
            &label(),
        )
        .expect("ok");
        let repo = parse_repo_config(
            "[board]\nkind = \"jira\"\nproject_key = \"A\"\n[remote]\nkind = \"github\"\nowner = \"o\"\nrepo = \"r\"\n",
            &label(),
        )
        .expect("ok");
        let s = resolve(&user, Some(repo), &root).expect("settings");
        assert_eq!(s.source, Source::RepoFile);
        assert_eq!(s.board.kind(), Kind::Jira);
        assert_eq!(s.board.credentials(), None);
    }

    #[test]
    /// CF-R-006, CF-E-004 — without a repository file the entry with the matching canonical path is used.
    fn ut_resolve_matches_user_entry_by_canonical_path() {
        let t = TempDir::new("canon");
        let root = t.path().join("first");
        std::fs::create_dir_all(&root).expect("mkdir");
        let user = parse_user_config(
            &USER.replace("/first", &root.display().to_string()),
            &label(),
        )
        .expect("ok");
        let via_dot = root.join("sub").join("..");
        std::fs::create_dir_all(root.join("sub")).expect("mkdir");
        let s = resolve(&user, None, &via_dot).expect("settings");
        assert_eq!(s.source, Source::UserFile);
        assert_eq!(s.board.credentials(), Some("gh"));
    }

    #[test]
    /// CF-R-007 — no repository file and no matching entry yields no settings.
    fn ut_resolve_none_when_unconfigured() {
        let user = parse_user_config(USER, &label()).expect("ok");
        assert_eq!(resolve(&user, None, Path::new("/other")), None);
    }

    #[test]
    /// CF-R-028 — upsert replaces the matching entry in place and appends otherwise, keeping order.
    fn ut_upsert_replaces_in_place_or_appends() {
        let mut user = parse_user_config(USER, &label()).expect("ok");
        upsert_repo(&mut user, Path::new("/second"), board(2), remote());
        upsert_repo(&mut user, Path::new("/first"), board(9), remote());
        assert_eq!(user.repo.len(), 2);
        assert_eq!(user.repo[0].path, PathBuf::from("/first"));
        assert_eq!(user.repo[0].board, board(9));
        assert_eq!(user.repo[1].path, PathBuf::from("/second"));
        assert_eq!(
            entry_for(&user, Path::new("/second")).map(|e| &e.board),
            Some(&board(2))
        );
    }

    #[test]
    /// CF-R-028, CF-R-034 — saving creates parent directories and round-trips the schema.
    fn ut_save_user_config_round_trips() {
        let t = TempDir::new("save");
        let path = t.path().join("deep").join("cfg.toml");
        let mut user = UserConfig::default();
        user.credentials
            .insert("gh".into(), Profile::Github { token: "t".into() });
        upsert_repo(&mut user, Path::new("/r"), board(4), remote());
        save_user_config(&path, &user).expect("saved");
        let back = load_user_config(&path).expect("loads");
        assert_eq!(back, user);
    }

    #[test]
    /// CF-R-033 — the repository-level file carries every key except `credentials`.
    fn ut_save_repo_config_strips_credentials() {
        let t = TempDir::new("saverepo");
        let path = t.path().join(".prodgy.toml");
        let mut b = board(4);
        b.set_credentials(Some("gh".into()));
        let mut r = remote();
        r.set_credentials(Some("bb".into()));
        save_repo_config(&path, &b, &r).expect("saved");
        let text = std::fs::read_to_string(&path).expect("readable");
        assert!(!text.contains("credentials"), "{text}");
        let back = load_repo_config(&path).expect("loads").expect("present");
        assert_eq!(back.board.identifiers(), b.identifiers());
        assert_eq!(back.remote.identifiers(), r.identifiers());
    }

    #[test]
    /// CF-R-035 — an unwritable target reports an error instead of panicking.
    fn ut_save_failure_is_error() {
        let t = TempDir::new("ro");
        let blocker = t.write("file", "");
        let path = blocker.join("cfg.toml");
        let err = save_user_config(&path, &UserConfig::default()).expect_err("fails");
        assert!(matches!(err, ConfigError::Io { .. }), "{err}");
    }

    #[test]
    /// CF-E-002 — a present but malformed repository file is an error, not a fallback.
    fn ut_malformed_repo_file_is_error() {
        let t = TempDir::new("badrepo");
        let path = t.write(".prodgy.toml", "[board]\nkind = \"nope\"\n");
        let err = load_repo_config(&path).expect_err("fails");
        assert!(matches!(err, ConfigError::Parse { .. }), "{err}");
        assert_eq!(
            load_repo_config(&t.path().join("absent.toml")).expect("ok"),
            None
        );
    }
}
