//! Where the files live: repository root discovery and the two file locations.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Nearest ancestor of `start` (itself included) containing a `.git` entry.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// `$XDG_CONFIG_HOME/dovetail/config.toml`, or `$HOME/.config/dovetail/config.toml` when the
/// variable is unset or empty.
pub fn user_config_path(xdg_config_home: Option<&OsStr>, home: &Path) -> PathBuf {
    let base = match xdg_config_home {
        Some(xdg) if !xdg.is_empty() => PathBuf::from(xdg),
        _ => home.join(".config"),
    };
    base.join("dovetail").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::TempDir;

    #[test]
    /// CF-R-001 — the nearest ancestor with a `.git` entry is the root, the start itself included.
    fn ut_find_repo_root_walks_up_to_dot_git() {
        let t = TempDir::new("root");
        let root = t.path().join("proj");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).expect("mkdir");
        assert_eq!(find_repo_root(&nested), Some(root.clone()));
        assert_eq!(find_repo_root(&root), Some(root));
    }

    #[test]
    /// CF-R-001 — a `.git` file (worktree) counts as an entry too.
    fn ut_find_repo_root_accepts_dot_git_file() {
        let t = TempDir::new("gitfile");
        let root = t.path().join("wt");
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(root.join(".git"), "gitdir: /elsewhere").expect("write");
        assert_eq!(find_repo_root(&root), Some(root));
    }

    #[test]
    /// CF-R-002 — no `.git` in any ancestor yields `None`.
    fn ut_find_repo_root_none_outside_repository() {
        let t = TempDir::new("norepo");
        let dir = t.path().join("plain");
        std::fs::create_dir_all(&dir).expect("mkdir");
        assert_eq!(find_repo_root(&dir), None);
    }

    #[test]
    /// CF-R-003 — `XDG_CONFIG_HOME` set and non-empty wins.
    fn ut_user_config_path_prefers_xdg() {
        let p = user_config_path(Some(OsStr::new("/xdg")), Path::new("/home/u"));
        assert_eq!(p, PathBuf::from("/xdg/dovetail/config.toml"));
    }

    #[test]
    /// CF-R-003, CF-E-001 — unset or empty `XDG_CONFIG_HOME` falls back to `$HOME/.config`.
    fn ut_user_config_path_falls_back_to_home() {
        let home = Path::new("/home/u");
        assert_eq!(
            user_config_path(None, home),
            PathBuf::from("/home/u/.config/dovetail/config.toml")
        );
        assert_eq!(
            user_config_path(Some(OsStr::new("")), home),
            PathBuf::from("/home/u/.config/dovetail/config.toml")
        );
    }
}
