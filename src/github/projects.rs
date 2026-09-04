//! Listing an owner's Projects (v2) through the GraphQL API.

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const ENDPOINT: &str = "https://api.github.com/graphql";
const QUERY: &str = "query($login: String!) { repositoryOwner(login: $login) { ... on ProjectV2Owner { projectsV2(first: 100) { nodes { number title } } } } }";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub number: NonZeroU64,
    pub title: String,
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GithubError {
    #[error("github: {0}")]
    Http(#[from] reqwest::Error),
    #[error("github: HTTP {0}")]
    Status(u16),
    #[error("github: {0}")]
    Graphql(String),
    #[error("github: owner not found")]
    MissingOwner,
    #[error("github: project not found")]
    MissingProject,
    #[error("github: issue not found")]
    MissingIssue,
    #[error("github: {0}")]
    Decode(String),
}

#[derive(Serialize)]
struct Request<'a> {
    query: &'a str,
    variables: Variables<'a>,
}

#[derive(Serialize)]
struct Variables<'a> {
    login: &'a str,
}

#[derive(Deserialize)]
struct Response {
    data: Option<Data>,
    errors: Option<Vec<GraphqlError>>,
}

#[derive(Deserialize)]
struct GraphqlError {
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    repository_owner: Option<Owner>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Owner {
    projects_v2: Projects,
}

#[derive(Deserialize)]
struct Projects {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    number: NonZeroU64,
    title: String,
}

/// The JSON body sent to the GraphQL endpoint.
pub fn request_body(owner: &str) -> String {
    serde_json::to_string(&Request {
        query: QUERY,
        variables: Variables { login: owner },
    })
    .expect("a request of two strings serializes")
}

/// Projects from a GraphQL response body.
pub fn parse_projects(body: &str) -> Result<Vec<Project>, GithubError> {
    let response: Response =
        serde_json::from_str(body).map_err(|e| GithubError::Decode(e.to_string()))?;
    if let Some(first) = response
        .errors
        .and_then(|mut errors| errors.drain(..).next())
    {
        return Err(GithubError::Graphql(first.message));
    }
    let owner = response
        .data
        .and_then(|d| d.repository_owner)
        .ok_or(GithubError::MissingOwner)?;
    Ok(owner
        .projects_v2
        .nodes
        .into_iter()
        .map(|n| Project {
            number: n.number,
            title: n.title,
        })
        .collect())
}

pub async fn list_projects(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
) -> Result<Vec<Project>, GithubError> {
    let response = client
        .post(ENDPOINT)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, "prodgy")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request_body(owner))
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(GithubError::Status(status.as_u16()));
    }
    parse_projects(&response.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// GH-R-001, GH-R-003 — the request queries repositoryOwner's projectsV2, first 100, with the login variable.
    fn ut_request_body_shape() {
        let body: serde_json::Value = serde_json::from_str(&request_body("octo")).expect("json");
        let query = body["query"].as_str().expect("query");
        assert!(query.contains("repositoryOwner(login: $login)"), "{query}");
        assert!(query.contains("projectsV2(first: 100)"), "{query}");
        assert!(query.contains("number title"), "{query}");
        assert_eq!(body["variables"]["login"], "octo");
    }

    #[test]
    /// GH-R-001 — nodes map to projects with number and title.
    fn ut_parse_projects_from_nodes() {
        let body = r#"{"data":{"repositoryOwner":{"projectsV2":{"nodes":[{"number":3,"title":"Roadmap"},{"number":7,"title":"Bugs"}]}}}}"#;
        let projects = parse_projects(body).expect("parses");
        assert_eq!(
            projects,
            vec![
                Project {
                    number: NonZeroU64::new(3).expect("nz"),
                    title: "Roadmap".into()
                },
                Project {
                    number: NonZeroU64::new(7).expect("nz"),
                    title: "Bugs".into()
                },
            ]
        );
    }

    #[test]
    /// GH-R-002 — a GraphQL errors array is a typed error carrying the first message.
    fn ut_parse_graphql_errors() {
        let body = r#"{"data":null,"errors":[{"message":"Bad credentials"},{"message":"other"}]}"#;
        let err = parse_projects(body).expect_err("rejects");
        assert!(
            matches!(err, GithubError::Graphql(ref m) if m == "Bad credentials"),
            "{err}"
        );
    }

    #[test]
    /// GH-R-002 — a missing owner and an undecodable body are typed errors.
    fn ut_parse_missing_owner_and_garbage() {
        let err = parse_projects(r#"{"data":{"repositoryOwner":null}}"#).expect_err("rejects");
        assert!(matches!(err, GithubError::MissingOwner), "{err}");
        let err = parse_projects("<html>").expect_err("rejects");
        assert!(matches!(err, GithubError::Decode(_)), "{err}");
        let err = parse_projects("").expect_err("rejects");
        assert!(matches!(err, GithubError::Decode(_)), "{err}");
    }
}
