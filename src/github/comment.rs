//! Posting and updating a conversation comment or a subject's body on a pull request or an issue.

use serde::{Deserialize, Serialize};

use crate::github::GithubError;

const ENDPOINT: &str = "https://api.github.com/graphql";

const ADD: &str = "mutation($subject: ID!, $body: String!) { addComment(input: {subjectId: $subject, body: $body}) { commentEdge { node { id } } } }";

const UPDATE_COMMENT: &str = "mutation($id: ID!, $body: String!) { updateIssueComment(input: {id: $id, body: $body}) { issueComment { id } } }";

const UPDATE_ISSUE: &str = "mutation($id: ID!, $body: String!) { updateIssue(input: {id: $id, body: $body}) { issue { id } } }";

const UPDATE_PULL: &str = "mutation($id: ID!, $body: String!) { updatePullRequest(input: {pullRequestId: $id, body: $body}) { pullRequest { id } } }";

/// Which body an update replaces, each variant holding the node id its mutation takes.
// Constructed once the pending-edit state builds an `Edit` per box; only `update_body`
// matches on it so far.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditTarget {
    Comment { id: String },
    IssueBody { id: String },
    PullBody { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub target: EditTarget,
    pub body: String,
}

/// One `submit` of the conversation view: the draft to post, then the edits in box order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentAction {
    /// Node id of the issue or pull request, the `addComment` subject.
    pub subject_id: String,
    pub draft: Option<String>,
    pub edits: Vec<Edit>,
}

/// What the run achieved, whatever its outcome, so a retry sends only the rest.
#[derive(Debug)]
pub struct CommentResult {
    pub posted: bool,
    pub edited: usize,
    pub outcome: Result<(), GithubError>,
}

#[derive(Serialize)]
struct Request<'a, V: Serialize> {
    query: &'a str,
    variables: V,
}

fn body<V: Serialize>(query: &str, variables: V) -> String {
    serde_json::to_string(&Request { query, variables })
        .expect("a request of plain values serializes")
}

#[derive(Serialize)]
struct AddVariables<'a> {
    subject: &'a str,
    body: &'a str,
}

pub fn add_body(subject_id: &str, comment_body: &str) -> String {
    body(
        ADD,
        AddVariables {
            subject: subject_id,
            body: comment_body,
        },
    )
}

#[derive(Serialize)]
struct UpdateVariables<'a> {
    id: &'a str,
    body: &'a str,
}

/// The request that replaces an edit's target body.
pub fn update_body(edit: &Edit) -> String {
    let (query, id) = match &edit.target {
        EditTarget::Comment { id } => (UPDATE_COMMENT, id),
        EditTarget::IssueBody { id } => (UPDATE_ISSUE, id),
        EditTarget::PullBody { id } => (UPDATE_PULL, id),
    };
    body(
        query,
        UpdateVariables {
            id,
            body: &edit.body,
        },
    )
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
    add_comment: Option<Payload>,
    update_issue_comment: Option<CommentPayload>,
    update_issue: Option<IssuePayload>,
    update_pull_request: Option<PullPayload>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    comment_edge: Option<Edge>,
}

#[derive(Deserialize)]
struct Edge {
    node: Option<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentPayload {
    issue_comment: Option<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssuePayload {
    issue: Option<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullPayload {
    pull_request: Option<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
}

/// The mutation's node id out of the response: `addComment`'s, or an update's target.
pub fn parse_id(body: &str) -> Result<String, GithubError> {
    let response: Response =
        serde_json::from_str(body).map_err(|e| GithubError::Decode(e.to_string()))?;
    if let Some(first) = response
        .errors
        .and_then(|mut errors| errors.drain(..).next())
    {
        return Err(GithubError::Graphql(first.message));
    }
    let data = response
        .data
        .ok_or_else(|| GithubError::Decode("no data".into()))?;
    data.add_comment
        .and_then(|p| p.comment_edge)
        .and_then(|e| e.node)
        .or_else(|| data.update_issue_comment.and_then(|p| p.issue_comment))
        .or_else(|| data.update_issue.and_then(|p| p.issue))
        .or_else(|| data.update_pull_request.and_then(|p| p.pull_request))
        .map(|n| n.id)
        .ok_or_else(|| GithubError::Decode("no node in the mutation payload".into()))
}

async fn mutate(
    client: &reqwest::Client,
    token: &str,
    body: String,
) -> Result<String, GithubError> {
    let response = client
        .post(ENDPOINT)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, "dovetail")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(GithubError::Status(status.as_u16()));
    }
    parse_id(&response.text().await?)
}

/// Runs the draft post then every edit in order, stopping at the first failure but reporting
/// the progress made.
pub async fn run(client: &reqwest::Client, token: &str, action: CommentAction) -> CommentResult {
    let mut result = CommentResult {
        posted: false,
        edited: 0,
        outcome: Ok(()),
    };
    if let Some(draft) = action.draft {
        if let Err(e) = mutate(client, token, add_body(&action.subject_id, &draft)).await {
            result.outcome = Err(e);
            return result;
        }
        result.posted = true;
    }
    for edit in &action.edits {
        if let Err(e) = mutate(client, token, update_body(edit)).await {
            result.outcome = Err(e);
            return result;
        }
        result.edited += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// GH-R-024 — the request carries the mutation with the subject id and body as variables.
    fn ut_add_body() {
        let body = add_body("PR_1", "well **done**");
        let value: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert!(
            value["query"]
                .as_str()
                .expect("query")
                .contains("addComment(input: {subjectId: $subject, body: $body})"),
        );
        assert_eq!(value["variables"]["subject"], "PR_1");
        assert_eq!(value["variables"]["body"], "well **done**");
    }

    #[test]
    /// GH-R-026, GH-R-027, GH-R-028 — each edit target's mutation carries the target's id and the
    /// new body as variables.
    fn ut_update_body() {
        let comment = serde_json::from_str::<serde_json::Value>(&update_body(&Edit {
            target: EditTarget::Comment { id: "IC_1".into() },
            body: "edited".into(),
        }))
        .expect("json");
        assert!(
            comment["query"]
                .as_str()
                .expect("query")
                .contains("updateIssueComment(input: {id: $id, body: $body})")
        );
        assert_eq!(comment["variables"]["id"], "IC_1");
        assert_eq!(comment["variables"]["body"], "edited");

        let issue = serde_json::from_str::<serde_json::Value>(&update_body(&Edit {
            target: EditTarget::IssueBody { id: "I_1".into() },
            body: "new body".into(),
        }))
        .expect("json");
        assert!(
            issue["query"]
                .as_str()
                .expect("query")
                .contains("updateIssue(input: {id: $id, body: $body})")
        );
        assert_eq!(issue["variables"]["id"], "I_1");

        let pull = serde_json::from_str::<serde_json::Value>(&update_body(&Edit {
            target: EditTarget::PullBody { id: "PR_1".into() },
            body: "new body".into(),
        }))
        .expect("json");
        assert!(
            pull["query"]
                .as_str()
                .expect("query")
                .contains("updatePullRequest(input: {pullRequestId: $id, body: $body})")
        );
        assert_eq!(pull["variables"]["id"], "PR_1");
    }

    #[test]
    /// GH-R-024, GH-E-015, GH-E-016 — the response parses to the mutation's node id, an
    /// `addComment` or an update alike; a GraphQL error message becomes the typed error; a
    /// missing node is a decode error; truncated input never crashes.
    fn ut_parse_id() {
        let ok = r#"{"data":{"addComment":{"commentEdge":{"node":{"id":"IC_9"}}}}}"#;
        assert_eq!(parse_id(ok).expect("id"), "IC_9");
        let updated = r#"{"data":{"updateIssueComment":{"issueComment":{"id":"IC_1"}}}}"#;
        assert_eq!(parse_id(updated).expect("id"), "IC_1");
        let issue = r#"{"data":{"updateIssue":{"issue":{"id":"I_1"}}}}"#;
        assert_eq!(parse_id(issue).expect("id"), "I_1");
        let pull = r#"{"data":{"updatePullRequest":{"pullRequest":{"id":"PR_1"}}}}"#;
        assert_eq!(parse_id(pull).expect("id"), "PR_1");
        let locked = r#"{"data":null,"errors":[{"message":"Unlocked conversations only"}]}"#;
        assert!(matches!(
            parse_id(locked),
            Err(GithubError::Graphql(m)) if m == "Unlocked conversations only"
        ));
        let denied =
            r#"{"data":null,"errors":[{"message":"Resource not accessible by integration"}]}"#;
        assert!(matches!(
            parse_id(denied),
            Err(GithubError::Graphql(m)) if m == "Resource not accessible by integration"
        ));
        let empty = r#"{"data":{"addComment":null}}"#;
        assert!(matches!(parse_id(empty), Err(GithubError::Decode(_))));
        let empty_update = r#"{"data":{"updateIssue":null}}"#;
        assert!(matches!(
            parse_id(empty_update),
            Err(GithubError::Decode(_))
        ));
        assert!(matches!(
            parse_id(r#"{"data":{"addComm"#),
            Err(GithubError::Decode(_))
        ));
    }

    fn unreachable_client() -> reqwest::Client {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).expect("proxy url"))
            .build()
            .expect("client")
    }

    #[tokio::test]
    /// GH-R-026, GH-R-027, GH-R-028 — an unreachable host is a typed HTTP error; the draft is
    /// attempted before any edit and stops the run; with no draft the first edit is attempted
    /// and stops the run; the result reports exactly what was accepted (none, here).
    async fn ut_run_reports_progress() {
        let client = unreachable_client();
        let edits = vec![
            Edit {
                target: EditTarget::Comment { id: "IC_1".into() },
                body: "a".into(),
            },
            Edit {
                target: EditTarget::IssueBody { id: "I_1".into() },
                body: "b".into(),
            },
        ];
        let with_draft = run(
            &client,
            "t",
            CommentAction {
                subject_id: "N_1".into(),
                draft: Some("hi".into()),
                edits: edits.clone(),
            },
        )
        .await;
        assert!(
            matches!(with_draft.outcome, Err(GithubError::Http(_))),
            "{:?}",
            with_draft.outcome
        );
        assert_eq!((with_draft.posted, with_draft.edited), (false, 0));

        let without_draft = run(
            &client,
            "t",
            CommentAction {
                subject_id: "N_1".into(),
                draft: None,
                edits,
            },
        )
        .await;
        assert!(matches!(without_draft.outcome, Err(GithubError::Http(_))));
        assert_eq!((without_draft.posted, without_draft.edited), (false, 0));
    }
}
