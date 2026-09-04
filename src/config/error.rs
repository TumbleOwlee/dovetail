//! Load and write failures of the configuration files. Every variant renders as a single
//! line so `main` can print it verbatim before exiting.

use std::path::PathBuf;

use thiserror::Error;

use super::schema::Kind;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("not inside a git repository (no .git found from {0})")]
    NoRepository(PathBuf),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("{path}: duplicate [[repo]] path {repo}")]
    DuplicateRepoPath { path: PathBuf, repo: PathBuf },
    #[error("{path}: credentials reference \"{reference}\" names no profile")]
    UnknownProfile { path: PathBuf, reference: String },
    #[error(
        "{path}: credentials reference \"{reference}\" is a {profile_kind} profile but the section kind is {section_kind}"
    )]
    ProfileKindMismatch {
        path: PathBuf,
        reference: String,
        section_kind: Kind,
        profile_kind: Kind,
    },
    #[error("{path}: a repository-level file must not carry credentials")]
    RepoFileCredentials { path: PathBuf },
    #[error("{path}: {message}")]
    Serialize { path: PathBuf, message: String },
}
