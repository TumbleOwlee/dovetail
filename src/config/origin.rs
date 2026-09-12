//! Owner and repository derived from a git remote URL, for dialog placeholders.

use std::path::Path;

use super::schema::Kind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub host: Kind,
    pub owner: String,
    pub repo: String,
}

impl Origin {
    /// Parses `https://`, `ssh://` and scp-like `git@host:owner/repo(.git)` forms for
    /// `github.com` and `bitbucket.org`; anything else is `None`.
    pub fn parse(url: &str) -> Option<Origin> {
        let url = url.trim();
        // Strip scheme and userinfo: "https://user@host/path" and "ssh://git@host/path" both
        // reduce to "host/path"; the scp-like "git@host:path" reduces to "host/path" as well.
        let rest = match url.split_once("://") {
            Some((_, rest)) => rest,
            None => url,
        };
        let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
        // The host ends at whichever separator comes first: '/' for URL forms, ':' for scp-like.
        let split_at = rest.find(['/', ':'])?;
        let (host, path) = (&rest[..split_at], &rest[split_at + 1..]);
        let host = match host {
            "github.com" => Kind::Github,
            "bitbucket.org" => Kind::Bitbucket,
            _ => return None,
        };
        let mut parts = path.trim_matches('/').split('/');
        let owner = parts.next().filter(|s| !s.is_empty())?;
        let repo = parts
            .next()
            .filter(|s| !s.is_empty())?
            .trim_end_matches(".git");
        if repo.is_empty() {
            return None;
        }
        Some(Origin {
            host,
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    /// The `origin` remote of the repository at `root`, via `git config`.
    pub fn of_repo(root: &Path) -> Option<Origin> {
        let output = std::process::Command::new("git")
            .args(["config", "--get", "remote.origin.url"])
            .current_dir(root)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Origin::parse(&String::from_utf8_lossy(&output.stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::TempDir;

    fn gh(owner: &str, repo: &str) -> Option<Origin> {
        Some(Origin {
            host: Kind::Github,
            owner: owner.into(),
            repo: repo.into(),
        })
    }

    #[test]
    /// TU-R-009 — https, ssh and scp-like GitHub URLs yield owner and repo without `.git`.
    fn ut_parse_github_url_forms() {
        assert_eq!(
            Origin::parse("https://github.com/TumbleOwlee/dovetail"),
            gh("TumbleOwlee", "dovetail")
        );
        assert_eq!(Origin::parse("https://github.com/o/r.git"), gh("o", "r"));
        assert_eq!(Origin::parse("git@github.com:o/r.git"), gh("o", "r"));
        assert_eq!(Origin::parse("ssh://git@github.com/o/r.git"), gh("o", "r"));
        assert_eq!(Origin::parse("https://github.com/o/r/"), gh("o", "r"));
    }

    #[test]
    /// TU-R-009 — Bitbucket URLs yield workspace and slug as owner and repo.
    fn ut_parse_bitbucket_url() {
        let o = Origin::parse("git@bitbucket.org:acme/service.git").expect("parses");
        assert_eq!(o.host, Kind::Bitbucket);
        assert_eq!((o.owner.as_str(), o.repo.as_str()), ("acme", "service"));
        assert_eq!(
            Origin::parse("https://acme@bitbucket.org/acme/service.git").map(|o| o.owner),
            Some("acme".into())
        );
    }

    #[test]
    /// TU-E-003 — other hosts and malformed URLs yield nothing.
    fn ut_parse_rejects_other_hosts_and_garbage() {
        assert_eq!(Origin::parse("https://gitlab.com/o/r.git"), None);
        assert_eq!(Origin::parse("https://github.com/only-owner"), None);
        assert_eq!(Origin::parse(""), None);
        assert_eq!(Origin::parse("not a url"), None);
    }

    #[test]
    /// TU-E-003 — a repository without an `origin` remote yields nothing.
    fn ut_of_repo_without_origin_is_none() {
        let t = TempDir::new("noorigin");
        let root = t.path().join("r");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        std::fs::write(root.join(".git").join("config"), "[core]\n\tbare = false\n")
            .expect("write");
        assert_eq!(Origin::of_repo(&root), None);
    }
}
