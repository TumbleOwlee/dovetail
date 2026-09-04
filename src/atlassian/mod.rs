//! Atlassian integration: Jira issues and Bitbucket pull requests through the REST APIs.

pub mod projects;

pub use projects::{AtlassianError, JiraProject};
