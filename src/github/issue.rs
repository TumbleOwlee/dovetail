//! One issue's details by node id.

use serde::{Deserialize, Serialize};

use super::board::{Assignee, Label, LabelNode, Nodes};
use super::projects::GithubError;
use super::timeline::{self, TimelineItem};

const ENDPOINT: &str = "https://api.github.com/graphql";

const QUERY_HEAD: &str = "query($id: ID!, $after: String) { node(id: $id) { __typename ... on Issue { title number state body url author { login } repository { nameWithOwner } labels(first: 10) { nodes { name color } } assignees(first: 5) { nodes { login } } projectItems(first: 10) { nodes { project { title } } } milestone { title } parent { number title } subIssues(first: 20) { nodes { number title } } closedByPullRequestsReferences(first: 10) { nodes { number title repository { nameWithOwner } } } participants(first: 20) { nodes { login } }";

const QUERY_TAIL: &str = " } } }";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum IssueState {
    #[default]
    Open,
    Closed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub state: IssueState,
    pub body: String,
    pub url: String,
    /// `None` when the author account was deleted.
    pub author: Option<String>,
    pub repository: String,
    pub labels: Vec<Label>,
    pub assignees: Vec<String>,
    pub projects: Vec<String>,
    pub milestone: Option<String>,
    /// `parent #<number> <title>` then `sub #<number> <title>` lines.
    pub relationships: Vec<String>,
    pub development: Vec<PullRef>,
    pub participants: Vec<String>,
    pub timeline: Vec<TimelineItem>,
}

/// A pull request that closes an issue; `repository` is its `nameWithOwner`, which may differ
/// from the issue's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRef {
    pub number: u64,
    pub title: String,
    pub repository: String,
}

/// One page of a details response: the issue with this page's timeline items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub issue: Issue,
    /// Cursor of the next timeline page, `None` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
struct Request<'a> {
    query: &'a str,
    variables: Variables<'a>,
}

#[derive(Serialize)]
struct Variables<'a> {
    id: &'a str,
    after: Option<&'a str>,
}

/// The query with the issue timeline selection in place.
fn query() -> String {
    format!(
        "{QUERY_HEAD} {} {QUERY_TAIL}",
        timeline::selection(timeline::Owner::Issue)
    )
}

/// The JSON body sent to the GraphQL endpoint.
pub fn request_body(id: &str, after: Option<&str>) -> String {
    serde_json::to_string(&Request {
        query: &query(),
        variables: Variables { id, after },
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
    node: Option<Node>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Node {
    Issue(Box<IssueNode>),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueNode {
    title: String,
    number: u64,
    state: IssueState,
    body: String,
    url: String,
    author: Option<Author>,
    repository: Repository,
    labels: Nodes<LabelNode>,
    assignees: Nodes<Assignee>,
    project_items: Nodes<ProjectItem>,
    milestone: Option<Titled>,
    parent: Option<Ref>,
    sub_issues: Nodes<Ref>,
    closed_by_pull_requests_references: Nodes<PullRefNode>,
    participants: Nodes<Assignee>,
    timeline_items: timeline::Connection,
}

#[derive(Deserialize)]
struct ProjectItem {
    project: Titled,
}

#[derive(Deserialize)]
struct Titled {
    title: String,
}

#[derive(Deserialize)]
struct Ref {
    number: u64,
    title: String,
}

#[derive(Deserialize)]
struct PullRefNode {
    number: u64,
    title: String,
    repository: Repository,
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    name_with_owner: String,
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
    let Some(Node::Issue(node)) = response.data.and_then(|d| d.node) else {
        return Err(GithubError::MissingIssue);
    };
    let next_cursor = node.timeline_items.next_cursor();
    let reference = |prefix: &str, r: Ref| format!("{prefix}#{} {}", r.number, r.title);
    let mut relationships: Vec<String> = node
        .parent
        .into_iter()
        .map(|r| reference("parent ", r))
        .collect();
    relationships.extend(
        node.sub_issues
            .nodes
            .into_iter()
            .map(|r| reference("sub ", r)),
    );
    Ok(Page {
        issue: Issue {
            number: node.number,
            title: node.title,
            state: node.state,
            body: node.body,
            url: node.url,
            author: node.author.map(|a| a.login),
            repository: node.repository.name_with_owner,
            labels: node
                .labels
                .nodes
                .into_iter()
                .map(|l| Label {
                    name: l.name,
                    color: l.color,
                })
                .collect(),
            assignees: node.assignees.nodes.into_iter().map(|a| a.login).collect(),
            projects: node
                .project_items
                .nodes
                .into_iter()
                .map(|p| p.project.title)
                .collect(),
            milestone: node.milestone.map(|m| m.title),
            relationships,
            development: node
                .closed_by_pull_requests_references
                .nodes
                .into_iter()
                .map(|r| PullRef {
                    number: r.number,
                    title: r.title,
                    repository: r.repository.name_with_owner,
                })
                .collect(),
            participants: node
                .participants
                .nodes
                .into_iter()
                .map(|a| a.login)
                .collect(),
            timeline: node.timeline_items.items(),
        },
        next_cursor,
    })
}

/// The issue from a single-page GraphQL response body.
#[cfg(test)]
pub fn parse_issue(body: &str) -> Result<Issue, GithubError> {
    parse_page(body).map(|p| p.issue)
}

pub async fn load_issue(
    client: &reqwest::Client,
    token: &str,
    id: &str,
) -> Result<Issue, GithubError> {
    let mut issue: Option<Issue> = None;
    let mut cursor: Option<String> = None;
    loop {
        let response = client
            .post(ENDPOINT)
            .bearer_auth(token)
            .header(reqwest::header::USER_AGENT, "prodgy")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request_body(id, cursor.as_deref()))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(GithubError::Status(status.as_u16()));
        }
        let page = parse_page(&response.text().await?)?;
        match issue.as_mut() {
            Some(issue) => issue.timeline.extend(page.issue.timeline),
            None => issue = Some(page.issue),
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(issue.expect("the first page was stored above"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"data":{"node":{"__typename":"Issue","title":"Crash on start","number":7,"state":"OPEN","body":"Steps:\n1. run\n2. boom","url":"https://github.com/o/r/issues/7","author":{"login":"octo"},"repository":{"nameWithOwner":"o/r"},"labels":{"nodes":[{"name":"bug","color":"d73a4a"}]},"assignees":{"nodes":[{"login":"a"}]},
        "projectItems":{"nodes":[{"project":{"title":"Roadmap"}}]},"milestone":{"title":"v1"},
        "parent":{"number":3,"title":"Epic"},"subIssues":{"nodes":[{"number":8,"title":"Child"}]},
        "closedByPullRequestsReferences":{"nodes":[{"number":5,"title":"Fix crash","repository":{"nameWithOwner":"o/r"}}]},"participants":{"nodes":[{"login":"octo"},{"login":"a"}]},
        "timelineItems":{"pageInfo":{"hasNextPage":true,"endCursor":"cur"},"nodes":[{"__typename":"IssueComment","body":"LGTM","createdAt":"2026-09-04T10:00:00Z","author":{"login":"a"}},{"__typename":"ClosedEvent","actor":{"login":"octo"},"createdAt":"2026-09-05T10:00:00Z","stateReason":"COMPLETED"}]}}}}"#;

    #[test]
    /// GH-R-010 — the query asks for the node by id with every detail field.
    fn ut_request_body_shape() {
        let body: serde_json::Value =
            serde_json::from_str(&request_body("I_1", Some("abc"))).expect("json");
        assert_eq!(body["variables"]["id"], "I_1");
        assert_eq!(body["variables"]["after"], "abc");
        let query = body["query"].as_str().expect("query");
        assert!(
            !query.contains("MERGED_EVENT") && !query.contains("PULL_REQUEST_REVIEW"),
            "{query}"
        );
        for part in [
            "node(id: $id)",
            "... on Issue",
            "state",
            "body",
            "author { login }",
            "repository { nameWithOwner }",
            "labels(first: 10)",
            "assignees(first: 5)",
            "projectItems(first: 10) { nodes { project { title } } }",
            "milestone { title }",
            "parent { number title }",
            "subIssues(first: 20) { nodes { number title } }",
            "closedByPullRequestsReferences(first: 10) { nodes { number title repository { nameWithOwner } } }",
            "participants(first: 20) { nodes { login } }",
            "timelineItems(first: 100, after: $after, itemTypes: [ISSUE_COMMENT, ",
            "... on ClosedEvent { actor { login } createdAt stateReason }",
        ] {
            assert!(query.contains(part), "{part} missing in {query}");
        }
    }

    #[test]
    /// GH-R-010 — every detail field is carried; a deleted author is `None`.
    fn ut_parse_issue_details() {
        let issue = parse_issue(BODY).expect("parses");
        assert_eq!(issue.number, 7);
        assert_eq!(issue.title, "Crash on start");
        assert_eq!(issue.state, IssueState::Open);
        assert_eq!(issue.body, "Steps:\n1. run\n2. boom");
        assert_eq!(issue.url, "https://github.com/o/r/issues/7");
        assert_eq!(issue.author.as_deref(), Some("octo"));
        assert_eq!(issue.repository, "o/r");
        assert_eq!(issue.labels[0].name, "bug");
        assert_eq!(issue.assignees, vec!["a".to_string()]);
        let ghost = BODY.replace(r#""author":{"login":"octo"}"#, r#""author":null"#);
        let issue = parse_issue(&ghost).expect("parses");
        assert_eq!(issue.author, None);
        let closed = BODY.replace(r#""state":"OPEN""#, r#""state":"CLOSED""#);
        assert_eq!(
            parse_issue(&closed).expect("parses").state,
            IssueState::Closed
        );
    }

    #[test]
    /// GH-R-010, GH-R-017 — sidebar fields and timeline items are carried; the cursor follows `hasNextPage`; missing parent and milestone are absent.
    fn ut_parse_sidebar_and_timeline() {
        let page = parse_page(BODY).expect("parses");
        assert_eq!(page.next_cursor.as_deref(), Some("cur"));
        let issue = page.issue;
        assert_eq!(issue.projects, vec!["Roadmap".to_string()]);
        assert_eq!(issue.milestone.as_deref(), Some("v1"));
        assert_eq!(
            issue.relationships,
            vec!["parent #3 Epic".to_string(), "sub #8 Child".to_string()]
        );
        assert_eq!(
            issue.development,
            vec![PullRef {
                number: 5,
                title: "Fix crash".into(),
                repository: "o/r".into()
            }]
        );
        assert_eq!(
            issue.participants,
            vec!["octo".to_string(), "a".to_string()]
        );
        assert_eq!(issue.timeline.len(), 2);
        assert_eq!(issue.timeline[0].actor.as_deref(), Some("a"));
        assert_eq!(
            issue.timeline[0].event,
            timeline::Event::Comment {
                body: "LGTM".into()
            }
        );
        assert_eq!(
            issue.timeline[1].event,
            timeline::Event::Closed {
                reason: Some("COMPLETED".into())
            }
        );
        let bare = BODY
            .replace(r#""milestone":{"title":"v1"}"#, r#""milestone":null"#)
            .replace(
                r#""parent":{"number":3,"title":"Epic"}"#,
                r#""parent":null"#,
            )
            .replace(r#""hasNextPage":true"#, r#""hasNextPage":false"#);
        let page = parse_page(&bare).expect("parses");
        assert_eq!(page.next_cursor, None);
        assert_eq!(page.issue.milestone, None);
        assert_eq!(page.issue.relationships, vec!["sub #8 Child".to_string()]);
    }

    #[test]
    /// GH-R-011 — a missing node, a non-issue node, a GraphQL error and garbage are typed errors.
    fn ut_errors_are_typed() {
        assert!(matches!(
            parse_issue(r#"{"data":{"node":null}}"#),
            Err(GithubError::MissingIssue)
        ));
        assert!(matches!(
            parse_issue(r#"{"data":{"node":{"__typename":"PullRequest"}}}"#),
            Err(GithubError::MissingIssue)
        ));
        assert!(matches!(
            parse_issue(r#"{"errors":[{"message":"nope"}]}"#),
            Err(GithubError::Graphql(m)) if m == "nope"
        ));
        assert!(matches!(parse_issue("{"), Err(GithubError::Decode(_))));
        assert!(matches!(
            parse_issue(r#"{"data":{"node":{"__typename":"Issue","title":"x"}}}"#),
            Err(GithubError::Decode(_))
        ));
    }
}
