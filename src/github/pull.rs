//! One pull request's details and comments.

use serde::{Deserialize, Serialize};

use super::projects::GithubError;
use super::pulls::PullState;

const ENDPOINT: &str = "https://api.github.com/graphql";

const QUERY: &str = "query($owner: String!, $name: String!, $number: Int!, $after: String) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { number title body state isDraft url author { login } comments(first: 100, after: $after) { pageInfo { hasNextPage endCursor } nodes { body createdAt author { login } } } } } }";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// `None` when the author account was deleted.
    pub author: Option<String>,
    /// ISO 8601 as GitHub sends it.
    pub created_at: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullDetails {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: PullState,
    pub draft: bool,
    pub url: String,
    pub author: Option<String>,
    pub comments: Vec<Comment>,
}

/// One page of a details response: the details with this page's comments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub details: PullDetails,
    /// Cursor of the next comments page, `None` on the last page.
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
    number: u64,
    after: Option<&'a str>,
}

/// The JSON body sent to the GraphQL endpoint.
pub fn request_body(owner: &str, repo: &str, number: u64, after: Option<&str>) -> String {
    serde_json::to_string(&Request {
        query: QUERY,
        variables: Variables {
            owner,
            name: repo,
            number,
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
    pull_request: Option<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    number: u64,
    title: String,
    body: String,
    state: PullState,
    is_draft: bool,
    url: String,
    author: Option<Author>,
    comments: Connection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    page_info: PageInfo,
    nodes: Vec<CommentNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentNode {
    body: String,
    created_at: String,
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
    let node = repository
        .pull_request
        .ok_or(GithubError::MissingPullRequest)?;
    let next_cursor = Some(node.comments.page_info)
        .filter(|p| p.has_next_page)
        .and_then(|p| p.end_cursor);
    let comments = node
        .comments
        .nodes
        .into_iter()
        .map(|c| Comment {
            author: c.author.map(|a| a.login),
            created_at: c.created_at,
            body: c.body,
        })
        .collect();
    Ok(Page {
        details: PullDetails {
            number: node.number,
            title: node.title,
            body: node.body,
            state: node.state,
            draft: node.is_draft,
            url: node.url,
            author: node.author.map(|a| a.login),
            comments,
        },
        next_cursor,
    })
}

pub async fn load_pull_request(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<PullDetails, GithubError> {
    let mut details: Option<PullDetails> = None;
    let mut cursor: Option<String> = None;
    loop {
        let response = client
            .post(ENDPOINT)
            .bearer_auth(token)
            .header(reqwest::header::USER_AGENT, "prodgy")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request_body(owner, repo, number, cursor.as_deref()))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(GithubError::Status(status.as_u16()));
        }
        let page = parse_page(&response.text().await?)?;
        match details.as_mut() {
            Some(details) => details.comments.extend(page.details.comments),
            None => details = Some(page.details),
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(details.expect("the first page was stored above"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"data":{"repository":{"pullRequest":{"number":5,"title":"Fix crash","body":"Fixes #7","state":"OPEN","isDraft":false,"url":"https://github.com/o/r/pull/5","author":{"login":"octo"},"comments":{"pageInfo":{"hasNextPage":true,"endCursor":"cur"},"nodes":[
        {"body":"LGTM","createdAt":"2026-09-04T10:00:00Z","author":{"login":"a"}},
        {"body":"gone","createdAt":"2026-09-03T10:00:00Z","author":null}
    ]}}}}}"#;

    #[test]
    /// GH-R-015 — the query asks for the pull request by number with its comments page.
    fn ut_request_body_shape() {
        let body: serde_json::Value =
            serde_json::from_str(&request_body("o", "r", 5, Some("abc"))).expect("json");
        assert_eq!(body["variables"]["owner"], "o");
        assert_eq!(body["variables"]["name"], "r");
        assert_eq!(body["variables"]["number"], 5);
        assert_eq!(body["variables"]["after"], "abc");
        let query = body["query"].as_str().expect("query");
        for part in [
            "pullRequest(number: $number)",
            "number title body state isDraft url author { login }",
            "comments(first: 100, after: $after)",
            "pageInfo { hasNextPage endCursor }",
            "nodes { body createdAt author { login } }",
        ] {
            assert!(query.contains(part), "{part} missing in {query}");
        }
    }

    #[test]
    /// GH-R-015 — details and comments are carried, deleted authors are `None`, the cursor follows `hasNextPage`.
    fn ut_parse_page() {
        let page = parse_page(BODY).expect("parses");
        assert_eq!(page.next_cursor.as_deref(), Some("cur"));
        let d = page.details;
        assert_eq!(
            (d.number, d.title.as_str(), d.body.as_str()),
            (5, "Fix crash", "Fixes #7")
        );
        assert_eq!((d.state, d.draft), (PullState::Open, false));
        assert_eq!(d.url, "https://github.com/o/r/pull/5");
        assert_eq!(d.author.as_deref(), Some("octo"));
        assert_eq!(
            d.comments,
            vec![
                Comment {
                    author: Some("a".into()),
                    created_at: "2026-09-04T10:00:00Z".into(),
                    body: "LGTM".into()
                },
                Comment {
                    author: None,
                    created_at: "2026-09-03T10:00:00Z".into(),
                    body: "gone".into()
                }
            ]
        );
        let last = BODY.replace(r#""hasNextPage":true"#, r#""hasNextPage":false"#);
        assert_eq!(parse_page(&last).expect("parses").next_cursor, None);
    }

    #[test]
    /// GH-R-016 — a missing repository or pull request, a GraphQL error and garbage are typed errors.
    fn ut_errors_are_typed() {
        assert!(matches!(
            parse_page(r#"{"data":{"repository":null}}"#),
            Err(GithubError::MissingRepository)
        ));
        assert!(matches!(
            parse_page(r#"{"data":{"repository":{"pullRequest":null}}}"#),
            Err(GithubError::MissingPullRequest)
        ));
        assert!(matches!(
            parse_page(r#"{"errors":[{"message":"nope"}]}"#),
            Err(GithubError::Graphql(m)) if m == "nope"
        ));
        assert!(matches!(parse_page("{"), Err(GithubError::Decode(_))));
        assert!(matches!(
            parse_page(r#"{"data":{"repository":{"pullRequest":{"number":1}}}}"#),
            Err(GithubError::Decode(_))
        ));
    }
}
