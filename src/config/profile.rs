//! Credential profiles derived on write: name choice and in-place update.

use super::schema::{Profile, UserConfig};

/// Stores `profile` under the section's existing reference when it has one, else under the
/// first free name of `base`, `base-2`, `base-3`, …; returns the name used.
pub fn store_profile(
    user: &mut UserConfig,
    existing: Option<&str>,
    base: &str,
    profile: Profile,
) -> String {
    let name = match existing {
        Some(name) => name.to_string(),
        None => (1u32..)
            .map(|n| {
                if n == 1 {
                    base.to_string()
                } else {
                    format!("{base}-{n}")
                }
            })
            .find(|candidate| !user.credentials.contains_key(candidate))
            .expect("an unbounded counter reaches a free name"),
    };
    user.credentials.insert(name.clone(), profile);
    name
}

/// `board-<dir>` / `remote-<dir>` for the repository root's file name.
pub fn profile_base(section: &str, root: &std::path::Path) -> String {
    let dir = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{section}-{dir}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn gh(token: &str) -> Profile {
        Profile::Github {
            token: token.into(),
        }
    }

    #[test]
    /// CF-R-031 — the base name is the section name plus the root's file name.
    fn ut_profile_base_uses_root_file_name() {
        assert_eq!(
            profile_base("board", Path::new("/home/u/git/dovetail")),
            "board-dovetail"
        );
        assert_eq!(
            profile_base("remote", Path::new("/x/acme-service/")),
            "remote-acme-service"
        );
    }

    #[test]
    /// CF-R-029, CF-R-031 — a section without a reference gets the base name.
    fn ut_store_profile_uses_base_when_free() {
        let mut user = UserConfig::default();
        let name = store_profile(&mut user, None, "board-p", gh("t"));
        assert_eq!(name, "board-p");
        assert_eq!(user.credentials["board-p"], gh("t"));
    }

    #[test]
    /// CF-R-032 — a taken base name gets the first free numeric suffix.
    fn ut_store_profile_suffixes_taken_names() {
        let mut user = UserConfig::default();
        user.credentials.insert("board-p".into(), gh("a"));
        user.credentials.insert("board-p-2".into(), gh("b"));
        let name = store_profile(&mut user, None, "board-p", gh("c"));
        assert_eq!(name, "board-p-3");
        assert_eq!(user.credentials["board-p"], gh("a"));
        assert_eq!(user.credentials["board-p-3"], gh("c"));
    }

    #[test]
    /// CF-R-030 — an existing reference is updated in place, never duplicated.
    fn ut_store_profile_updates_existing_reference() {
        let mut user = UserConfig::default();
        user.credentials.insert("mine".into(), gh("old"));
        let name = store_profile(&mut user, Some("mine"), "board-p", gh("new"));
        assert_eq!(name, "mine");
        assert_eq!(user.credentials.len(), 1);
        assert_eq!(user.credentials["mine"], gh("new"));
    }
}
