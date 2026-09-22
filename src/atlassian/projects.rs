//! Listing a Jira site's projects.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JiraProject {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AtlassianError {
    #[error("jira: {0}")]
    Http(#[from] reqwest::Error),
    #[error("jira: HTTP {0}")]
    Status(u16),
    #[error("jira: {0}")]
    Decode(String),
}

#[derive(Deserialize)]
struct SearchPage {
    values: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    key: String,
    name: String,
}

/// The project search URL for a site, first page of 100; trailing slashes
/// on the base are stripped so the path never doubles the separator.
pub fn projects_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{base}/rest/api/3/project/search?maxResults=100")
}

/// Projects from a search response body.
pub fn parse_projects(body: &str) -> Result<Vec<JiraProject>, AtlassianError> {
    let page: SearchPage =
        serde_json::from_str(body).map_err(|e| AtlassianError::Decode(e.to_string()))?;
    Ok(page
        .values
        .into_iter()
        .map(|e| JiraProject {
            key: e.key,
            name: e.name,
        })
        .collect())
}

pub async fn list_projects(
    client: &reqwest::Client,
    base_url: &str,
    email: &str,
    token: &str,
) -> Result<Vec<JiraProject>, AtlassianError> {
    let response = client
        .get(projects_url(base_url))
        .basic_auth(email, Some(token))
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(AtlassianError::Status(status.as_u16()));
    }
    parse_projects(&response.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// AT-R-001, AT-R-003, AT-E-002 — the search URL is base + path with maxResults=100, trailing slashes on the base stripped.
    fn ut_projects_url() {
        assert_eq!(
            projects_url("https://acme.atlassian.net"),
            "https://acme.atlassian.net/rest/api/3/project/search?maxResults=100"
        );
        assert_eq!(
            projects_url("https://acme.atlassian.net/"),
            "https://acme.atlassian.net/rest/api/3/project/search?maxResults=100"
        );
        assert_eq!(
            projects_url("https://acme.atlassian.net//"),
            "https://acme.atlassian.net/rest/api/3/project/search?maxResults=100"
        );
    }

    #[test]
    /// AT-R-001 — `values` entries map to key and name; other fields are ignored.
    fn ut_parse_projects_values() {
        let body = r#"{"self":"x","maxResults":50,"startAt":0,"total":2,"isLast":true,"values":[{"expand":"","self":"u","id":"1","key":"ACME","name":"Acme","projectTypeKey":"software"},{"id":"2","key":"OPS","name":"Ops"}]}"#;
        let projects = parse_projects(body).expect("parses");
        assert_eq!(
            projects,
            vec![
                JiraProject {
                    key: "ACME".into(),
                    name: "Acme".into()
                },
                JiraProject {
                    key: "OPS".into(),
                    name: "Ops".into()
                },
            ]
        );
    }

    #[test]
    /// AT-R-002 — an undecodable body is a typed decode error.
    fn ut_parse_garbage_is_decode_error() {
        let err = parse_projects("<html>").expect_err("rejects");
        assert!(matches!(err, AtlassianError::Decode(_)), "{err}");
        let err = parse_projects(r#"{"values":[{"key":"A"}]}"#).expect_err("rejects");
        assert!(matches!(err, AtlassianError::Decode(_)), "{err}");
    }

    #[tokio::test]
    /// AT-R-002 — an unreachable site is a typed HTTP error, never a panic.
    async fn ut_list_projects_unreachable_is_http_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let client = reqwest::Client::new();
        let err = list_projects(&client, &format!("http://127.0.0.1:{port}"), "e", "t")
            .await
            .expect_err("connection refused");
        assert!(matches!(err, AtlassianError::Http(_)), "{err}");
    }
}
