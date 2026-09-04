//! GitHub integration: Projects, Issues and Pull Requests through the GitHub API.

pub mod projects;

pub use projects::{GithubError, Project};
