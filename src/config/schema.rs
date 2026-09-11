//! Serde shapes of the user-level and repository-level TOML files. Every state is a variant:
//! a section is exactly one kind with exactly that kind's fields, so an invalid combination
//! cannot be deserialized.

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU64;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Service a credential profile or a section talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Github,
    Jira,
    Bitbucket,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Github => "github",
            Kind::Jira => "jira",
            Kind::Bitbucket => "bitbucket",
        })
    }
}

/// One named entry of the user-level `credentials` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Profile {
    Github {
        token: String,
    },
    Jira {
        base_url: String,
        email: String,
        token: String,
    },
    Bitbucket {
        username: String,
        app_password: String,
    },
}

impl Profile {
    pub fn kind(&self) -> Kind {
        match self {
            Profile::Github { .. } => Kind::Github,
            Profile::Jira { .. } => Kind::Jira,
            Profile::Bitbucket { .. } => Kind::Bitbucket,
        }
    }
}

/// The `board` section of a repository entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Board {
    Github {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credentials: Option<String>,
        owner: String,
        repo: String,
        project: NonZeroU64,
    },
    Jira {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credentials: Option<String>,
        project_key: String,
    },
}

/// The `remote` section of a repository entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Remote {
    Github {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credentials: Option<String>,
        owner: String,
        repo: String,
    },
    Bitbucket {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credentials: Option<String>,
        workspace: String,
        repo: String,
    },
}

/// What `Board` and `Remote` have in common: a kind, an optional profile reference, and the
/// identifier keys shown in a tab summary.
pub trait Section {
    fn kind(&self) -> Kind;
    fn credentials(&self) -> Option<&str>;
    fn set_credentials(&mut self, credentials: Option<String>);
    /// Identifier keys and their values, in schema order, `credentials` and `kind` excluded.
    // Only test consumers remain since the main view stopped rendering the summary;
    // the allow lifts when a view lists section identifiers again.
    #[allow(dead_code)]
    fn identifiers(&self) -> Vec<(&'static str, String)>;
}

impl Section for Board {
    fn kind(&self) -> Kind {
        match self {
            Board::Github { .. } => Kind::Github,
            Board::Jira { .. } => Kind::Jira,
        }
    }

    fn credentials(&self) -> Option<&str> {
        match self {
            Board::Github { credentials, .. } | Board::Jira { credentials, .. } => {
                credentials.as_deref()
            }
        }
    }

    fn set_credentials(&mut self, value: Option<String>) {
        match self {
            Board::Github { credentials, .. } | Board::Jira { credentials, .. } => {
                *credentials = value;
            }
        }
    }

    fn identifiers(&self) -> Vec<(&'static str, String)> {
        match self {
            Board::Github {
                owner,
                repo,
                project,
                ..
            } => vec![
                ("owner", owner.clone()),
                ("repo", repo.clone()),
                ("project", project.to_string()),
            ],
            Board::Jira { project_key, .. } => vec![("project_key", project_key.clone())],
        }
    }
}

impl Section for Remote {
    fn kind(&self) -> Kind {
        match self {
            Remote::Github { .. } => Kind::Github,
            Remote::Bitbucket { .. } => Kind::Bitbucket,
        }
    }

    fn credentials(&self) -> Option<&str> {
        match self {
            Remote::Github { credentials, .. } | Remote::Bitbucket { credentials, .. } => {
                credentials.as_deref()
            }
        }
    }

    fn set_credentials(&mut self, value: Option<String>) {
        match self {
            Remote::Github { credentials, .. } | Remote::Bitbucket { credentials, .. } => {
                *credentials = value;
            }
        }
    }

    fn identifiers(&self) -> Vec<(&'static str, String)> {
        match self {
            Remote::Github { owner, repo, .. } => {
                vec![("owner", owner.clone()), ("repo", repo.clone())]
            }
            Remote::Bitbucket {
                workspace, repo, ..
            } => vec![("workspace", workspace.clone()), ("repo", repo.clone())],
        }
    }
}

/// One `[[repo]]` entry of the user-level file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoEntry {
    pub path: PathBuf,
    pub board: Board,
    pub remote: Remote,
}

/// The user-level file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credentials: BTreeMap<String, Profile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repo: Vec<RepoEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(n: u64) -> NonZeroU64 {
        NonZeroU64::new(n).expect("test constant is non-zero")
    }

    #[test]
    /// CF-R-010, CF-R-011, CF-R-012, CF-R-013 — profiles parse into their kind's variant.
    fn ut_profiles_parse_by_kind() {
        let text = r#"
[credentials.gh]
kind = "github"
token = "t"

[credentials.j]
kind = "jira"
base_url = "https://x.atlassian.net"
email = "e"
token = "t"

[credentials.bb]
kind = "bitbucket"
username = "u"
app_password = "p"
"#;
        let cfg: UserConfig = toml::from_str(text).expect("parses");
        assert_eq!(cfg.credentials["gh"], Profile::Github { token: "t".into() });
        assert_eq!(cfg.credentials["gh"].kind(), Kind::Github);
        assert_eq!(cfg.credentials["j"].kind(), Kind::Jira);
        assert_eq!(cfg.credentials["bb"].kind(), Kind::Bitbucket);
    }

    #[test]
    /// CF-R-014, CF-R-015, CF-R-016, CF-R-018, CF-R-020 — a repo entry parses with GitHub sections.
    fn ut_repo_entry_github_sections_parse() {
        let text = r#"
[[repo]]
path = "/r"
[repo.board]
kind = "github"
credentials = "gh"
owner = "o"
repo = "r"
project = 3
[repo.remote]
kind = "github"
owner = "o"
repo = "r"
"#;
        let cfg: UserConfig = toml::from_str(text).expect("parses");
        assert_eq!(cfg.repo.len(), 1);
        assert_eq!(
            cfg.repo[0].board,
            Board::Github {
                credentials: Some("gh".into()),
                owner: "o".into(),
                repo: "r".into(),
                project: nz(3)
            }
        );
        assert_eq!(cfg.repo[0].board.kind(), Kind::Github);
        assert_eq!(cfg.repo[0].board.credentials(), Some("gh"));
        assert_eq!(cfg.repo[0].remote.credentials(), None);
    }

    #[test]
    /// CF-R-017, CF-R-019 — Jira board and Bitbucket remote sections parse.
    fn ut_jira_and_bitbucket_sections_parse() {
        let text = r#"
[[repo]]
path = "/r"
[repo.board]
kind = "jira"
project_key = "ACME"
[repo.remote]
kind = "bitbucket"
workspace = "w"
repo = "s"
"#;
        let cfg: UserConfig = toml::from_str(text).expect("parses");
        assert_eq!(
            cfg.repo[0].board,
            Board::Jira {
                credentials: None,
                project_key: "ACME".into()
            }
        );
        assert_eq!(
            cfg.repo[0].remote,
            Remote::Bitbucket {
                credentials: None,
                workspace: "w".into(),
                repo: "s".into()
            }
        );
        assert_eq!(cfg.repo[0].board.kind(), Kind::Jira);
        assert_eq!(cfg.repo[0].remote.kind(), Kind::Bitbucket);
    }

    #[test]
    /// CF-R-016 — `project` must be a positive integer.
    fn ut_project_zero_is_rejected() {
        let text = r#"
[[repo]]
path = "/r"
[repo.board]
kind = "github"
owner = "o"
repo = "r"
project = 0
[repo.remote]
kind = "github"
owner = "o"
repo = "r"
"#;
        assert!(toml::from_str::<UserConfig>(text).is_err());
    }

    #[test]
    /// CF-R-022 — an unknown key anywhere is rejected, naming the key.
    fn ut_unknown_key_is_rejected() {
        let top = "bogus = 1\n";
        let err = toml::from_str::<UserConfig>(top).expect_err("rejects");
        assert!(err.to_string().contains("bogus"), "{err}");
        let section = r#"
[[repo]]
path = "/r"
[repo.board]
kind = "jira"
project_key = "A"
extra = "x"
[repo.remote]
kind = "github"
owner = "o"
repo = "r"
"#;
        let err = toml::from_str::<UserConfig>(section).expect_err("rejects");
        assert!(err.to_string().contains("extra"), "{err}");
    }

    #[test]
    /// CF-R-008 — an empty document is an empty configuration.
    fn ut_empty_document_is_default() {
        let cfg: UserConfig = toml::from_str("").expect("parses");
        assert_eq!(cfg, UserConfig::default());
    }

    #[test]
    /// CF-R-016, CF-R-019 — identifiers list the kind's keys in schema order without credentials.
    fn ut_identifiers_exclude_kind_and_credentials() {
        let board = Board::Github {
            credentials: Some("x".into()),
            owner: "o".into(),
            repo: "r".into(),
            project: nz(7),
        };
        assert_eq!(
            board.identifiers(),
            vec![
                ("owner", "o".to_string()),
                ("repo", "r".to_string()),
                ("project", "7".to_string())
            ]
        );
        let remote = Remote::Bitbucket {
            credentials: None,
            workspace: "w".into(),
            repo: "s".into(),
        };
        assert_eq!(
            remote.identifiers(),
            vec![("workspace", "w".to_string()), ("repo", "s".to_string())]
        );
        let mut jira = Board::Jira {
            credentials: None,
            project_key: "K".into(),
        };
        jira.set_credentials(Some("j".into()));
        assert_eq!(jira.credentials(), Some("j"));
        assert_eq!(jira.identifiers(), vec![("project_key", "K".to_string())]);
    }

    #[test]
    /// CF-R-015 — kinds display in their TOML spelling.
    fn ut_kind_displays_lowercase() {
        assert_eq!(Kind::Github.to_string(), "github");
        assert_eq!(Kind::Jira.to_string(), "jira");
        assert_eq!(Kind::Bitbucket.to_string(), "bitbucket");
    }
}
