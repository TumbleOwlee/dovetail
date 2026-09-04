//! One issue's details by node id.

use serde::{Deserialize, Serialize};

use super::board::{Assignee, Label, LabelNode, Nodes};
use super::projects::GithubError;

const ENDPOINT: &str = "https://api.github.com/graphql";

const QUERY: &str = "query($id: ID!) { node(id: $id) { __typename ... on Issue { title number state body url author { login } repository { nameWithOwner } labels(first: 10) { nodes { name color } } assignees(first: 5) { nodes { login } } } } }";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

#[derive(Serialize)]
struct Request<'a> {
    query: &'a str,
    variables: Variables<'a>,
}

#[derive(Serialize)]
struct Variables<'a> {
    id: &'a str,
}

/// The JSON body sent to the GraphQL endpoint.
pub fn request_body(id: &str) -> String {
    serde_json::to_string(&Request {
        query: QUERY,
        variables: Variables { id },
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
    Issue(IssueNode),
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

/// The issue from a GraphQL response body.
pub fn parse_issue(body: &str) -> Result<Issue, GithubError> {
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
    Ok(Issue {
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
    })
}

pub async fn load_issue(
    client: &reqwest::Client,
    token: &str,
    id: &str,
) -> Result<Issue, GithubError> {
    let response = client
        .post(ENDPOINT)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, "prodgy")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request_body(id))
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(GithubError::Status(status.as_u16()));
    }
    parse_issue(&response.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"data":{"node":{"__typename":"Issue","title":"Crash on start","number":7,"state":"OPEN","body":"Steps:\n1. run\n2. boom","url":"https://github.com/o/r/issues/7","author":{"login":"octo"},"repository":{"nameWithOwner":"o/r"},"labels":{"nodes":[{"name":"bug","color":"d73a4a"}]},"assignees":{"nodes":[{"login":"a"}]}}}}"#;

    #[test]
    /// GH-R-010 — the query asks for the node by id with every detail field.
    fn ut_request_body_shape() {
        let body: serde_json::Value = serde_json::from_str(&request_body("I_1")).expect("json");
        assert_eq!(body["variables"]["id"], "I_1");
        let query = body["query"].as_str().expect("query");
        for part in [
            "node(id: $id)",
            "... on Issue",
            "state",
            "body",
            "author { login }",
            "repository { nameWithOwner }",
            "labels(first: 10)",
            "assignees(first: 5)",
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
