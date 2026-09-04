//! A repository's pull requests.

use std::cmp::Reverse;

use serde::{Deserialize, Serialize};

use super::projects::GithubError;

const ENDPOINT: &str = "https://api.github.com/graphql";

const QUERY: &str = "query($owner: String!, $name: String!, $after: String) { repository(owner: $owner, name: $name) { pullRequests(first: 100, after: $after, orderBy: {field: UPDATED_AT, direction: DESC}) { pageInfo { hasNextPage endCursor } nodes { number title state isDraft updatedAt headRefName baseRefName author { login } } } } }";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PullState {
    #[default]
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub state: PullState,
    pub draft: bool,
    /// `None` when the author account was deleted.
    pub author: Option<String>,
    pub head: String,
    pub base: String,
    /// ISO 8601 as GitHub sends it, e.g. `2026-09-04T10:00:00Z`.
    pub updated_at: String,
}

/// One page of a pull request response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub pulls: Vec<PullRequest>,
    /// Cursor of the next page, `None` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
struct Request<'a> {
    query: &'a str,
    variables: Variables<'a>,
}

#[derive(Serialize)]
struct Variables<'a> {
    owner: &'a str,
    name: &'a str,
    after: Option<&'a str>,
}

/// The JSON body sent to the GraphQL endpoint.
pub fn request_body(owner: &str, repo: &str, after: Option<&str>) -> String {
    serde_json::to_string(&Request {
        query: QUERY,
        variables: Variables {
            owner,
            name: repo,
            after,
        },
    })
    .expect("a request of plain values serializes")
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
struct Data {
    repository: Option<Repository>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    pull_requests: Connection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    page_info: PageInfo,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    number: u64,
    title: String,
    state: PullState,
    is_draft: bool,
    updated_at: String,
    head_ref_name: String,
    base_ref_name: String,
    author: Option<Author>,
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

/// The page from a GraphQL response body.
pub fn parse_page(body: &str) -> Result<Page, GithubError> {
    let response: Response =
        serde_json::from_str(body).map_err(|e| GithubError::Decode(e.to_string()))?;
    if let Some(first) = response
        .errors
        .and_then(|mut errors| errors.drain(..).next())
    {
        return Err(GithubError::Graphql(first.message));
    }
    let repository = response
        .data
        .and_then(|d| d.repository)
        .ok_or(GithubError::MissingRepository)?;
    let connection = repository.pull_requests;
    let next_cursor = Some(connection.page_info)
        .filter(|p| p.has_next_page)
        .and_then(|p| p.end_cursor);
    let pulls = connection
        .nodes
        .into_iter()
        .map(|n| PullRequest {
            number: n.number,
            title: n.title,
            state: n.state,
            draft: n.is_draft,
            author: n.author.map(|a| a.login),
            head: n.head_ref_name,
            base: n.base_ref_name,
            updated_at: n.updated_at,
        })
        .collect();
    Ok(Page { pulls, next_cursor })
}

/// Open first, then merged, then closed; most recently updated first within each group.
pub fn sort_pulls(pulls: &mut [PullRequest]) {
    pulls.sort_by(|a, b| {
        (a.state as u8, Reverse(&a.updated_at)).cmp(&(b.state as u8, Reverse(&b.updated_at)))
    });
}

pub async fn load_pull_requests(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    repo: &str,
) -> Result<Vec<PullRequest>, GithubError> {
    let mut pulls = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let response = client
            .post(ENDPOINT)
            .bearer_auth(token)
            .header(reqwest::header::USER_AGENT, "prodgy")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request_body(owner, repo, cursor.as_deref()))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(GithubError::Status(status.as_u16()));
        }
        let page = parse_page(&response.text().await?)?;
        pulls.extend(page.pulls);
        cursor = page.next_cursor;
        if cursor.is_none() {
            sort_pulls(&mut pulls);
            return Ok(pulls);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"data":{"repository":{"pullRequests":{"pageInfo":{"hasNextPage":true,"endCursor":"cur"},"nodes":[
        {"number":5,"title":"Fix crash","state":"OPEN","isDraft":false,"updatedAt":"2026-09-04T10:00:00Z","headRefName":"fix","baseRefName":"main","author":{"login":"octo"}},
        {"number":4,"title":"Old","state":"CLOSED","isDraft":false,"updatedAt":"2026-09-03T10:00:00Z","headRefName":"old","baseRefName":"main","author":null},
        {"number":3,"title":"Draft","state":"OPEN","isDraft":true,"updatedAt":"2026-09-02T10:00:00Z","headRefName":"wip","baseRefName":"main","author":{"login":"a"}},
        {"number":2,"title":"Shipped","state":"MERGED","isDraft":false,"updatedAt":"2026-09-01T10:00:00Z","headRefName":"feat","baseRefName":"main","author":{"login":"b"}}
    ]}}}}"#;

    fn pull(number: u64, state: PullState, updated_at: &str) -> PullRequest {
        PullRequest {
            number,
            state,
            updated_at: updated_at.into(),
            ..PullRequest::default()
        }
    }

    #[test]
    /// GH-R-012 — the query asks for the repository's pull requests by update time with a cursor.
    fn ut_request_body_shape() {
        let body: serde_json::Value =
            serde_json::from_str(&request_body("o", "r", Some("abc"))).expect("json");
        assert_eq!(body["variables"]["owner"], "o");
        assert_eq!(body["variables"]["name"], "r");
        assert_eq!(body["variables"]["after"], "abc");
        let query = body["query"].as_str().expect("query");
        for part in [
            "repository(owner: $owner, name: $name)",
            "pullRequests(first: 100, after: $after, orderBy: {field: UPDATED_AT, direction: DESC})",
            "pageInfo { hasNextPage endCursor }",
            "number title state isDraft updatedAt headRefName baseRefName author { login }",
        ] {
            assert!(query.contains(part), "{part} missing in {query}");
        }
    }

    #[test]
    /// GH-R-012 — every field is carried, a deleted author is `None`, the cursor follows `hasNextPage`.
    fn ut_parse_page() {
        let page = parse_page(BODY).expect("parses");
        assert_eq!(page.next_cursor.as_deref(), Some("cur"));
        assert_eq!(page.pulls.len(), 4);
        assert_eq!(
            page.pulls[0],
            PullRequest {
                number: 5,
                title: "Fix crash".into(),
                state: PullState::Open,
                draft: false,
                author: Some("octo".into()),
                head: "fix".into(),
                base: "main".into(),
                updated_at: "2026-09-04T10:00:00Z".into(),
            }
        );
        assert_eq!(page.pulls[1].author, None);
        assert_eq!(page.pulls[1].state, PullState::Closed);
        assert!(page.pulls[2].draft);
        assert_eq!(page.pulls[3].state, PullState::Merged);
        let last = BODY.replace(r#""hasNextPage":true"#, r#""hasNextPage":false"#);
        assert_eq!(parse_page(&last).expect("parses").next_cursor, None);
    }

    #[test]
    /// GH-R-013 — open, then merged, then closed; newest update first inside each group.
    fn ut_sort_order() {
        let mut pulls = vec![
            pull(1, PullState::Closed, "2026-09-09T00:00:00Z"),
            pull(2, PullState::Merged, "2026-09-01T00:00:00Z"),
            pull(3, PullState::Open, "2026-09-02T00:00:00Z"),
            pull(4, PullState::Merged, "2026-09-05T00:00:00Z"),
            pull(5, PullState::Open, "2026-09-08T00:00:00Z"),
            pull(6, PullState::Closed, "2026-09-03T00:00:00Z"),
        ];
        sort_pulls(&mut pulls);
        let order: Vec<u64> = pulls.iter().map(|p| p.number).collect();
        assert_eq!(order, vec![5, 3, 4, 2, 1, 6]);
    }

    #[test]
    /// GH-R-014 — a missing repository, a GraphQL error and garbage are typed errors.
    fn ut_errors_are_typed() {
        assert!(matches!(
            parse_page(r#"{"data":{"repository":null}}"#),
            Err(GithubError::MissingRepository)
        ));
        assert!(matches!(
            parse_page(r#"{"errors":[{"message":"nope"}]}"#),
            Err(GithubError::Graphql(m)) if m == "nope"
        ));
        assert!(matches!(parse_page("{"), Err(GithubError::Decode(_))));
        assert!(matches!(
            parse_page(r#"{"data":{"repository":{"pullRequests":{"nodes":[{"number":1}]}}}}"#),
            Err(GithubError::Decode(_))
        ));
    }
}
