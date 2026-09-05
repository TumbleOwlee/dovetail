//! Submitting and discarding a pull request review through GraphQL mutations.
// Consumed once the pull request overlay has a comment prompt and a command line to drive the
// review flow, which wait on the editor dialog and command line widgets of ferrowl-ui #326.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::github::GithubError;

const ENDPOINT: &str = "https://api.github.com/graphql";

const CREATE: &str = "mutation($pull: ID!, $commit: GitObjectID!) { addPullRequestReview(input: {pullRequestId: $pull, commitOID: $commit}) { pullRequestReview { id } } }";

const THREAD: &str = "mutation($review: ID!, $path: String!, $body: String!, $line: Int!, $side: DiffSide!, $startLine: Int, $startSide: DiffSide) { addPullRequestReviewThread(input: {pullRequestReviewId: $review, path: $path, body: $body, line: $line, side: $side, startLine: $startLine, startSide: $startSide}) { thread { id } } }";

const SUBMIT: &str = "mutation($review: ID!, $event: PullRequestReviewEvent!, $body: String) { submitPullRequestReview(input: {pullRequestReviewId: $review, event: $event, body: $body}) { pullRequestReview { id } } }";

const DELETE: &str = "mutation($review: ID!) { deletePullRequestReview(input: {pullRequestReviewId: $review}) { pullRequestReview { id } } }";

/// Which state of the file a comment addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Side {
    /// The old state.
    Left,
    /// The new state.
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    Approve,
    RequestChanges,
    Comment,
}

/// One comment on a line or a range of lines of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub path: String,
    pub side: Side,
    /// First line of a range; equal to `line` for a single line.
    pub start_line: u32,
    /// Last line, or the only one.
    pub line: u32,
    pub body: String,
}

/// What to do with a review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    Submit {
        pull_id: String,
        head_oid: String,
        /// A review created by an earlier failed attempt, to continue with.
        review_id: Option<String>,
        /// Comments not yet sent.
        threads: Vec<Thread>,
        verdict: Verdict,
        body: String,
    },
    Discard {
        review_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOutcome {
    Submitted,
    Discarded,
}

/// The progress an action made, whatever its outcome: the review id in play and the number of
/// threads added, so a failure can be continued or discarded.
#[derive(Debug)]
pub struct ReviewResult {
    pub review_id: Option<String>,
    pub added: usize,
    pub outcome: Result<ReviewOutcome, GithubError>,
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
struct CreateVariables<'a> {
    pull: &'a str,
    commit: &'a str,
}

pub fn create_body(pull_id: &str, head_oid: &str) -> String {
    body(
        CREATE,
        CreateVariables {
            pull: pull_id,
            commit: head_oid,
        },
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadVariables<'a> {
    review: &'a str,
    path: &'a str,
    body: &'a str,
    line: u32,
    side: Side,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_side: Option<Side>,
}

/// A single line sends `line` and `side` only; a range adds its first line as `startLine`.
pub fn thread_body(review_id: &str, thread: &Thread) -> String {
    let range = thread.start_line < thread.line;
    body(
        THREAD,
        ThreadVariables {
            review: review_id,
            path: &thread.path,
            body: &thread.body,
            line: thread.line,
            side: thread.side,
            start_line: range.then_some(thread.start_line),
            start_side: range.then_some(thread.side),
        },
    )
}

#[derive(Serialize)]
struct SubmitVariables<'a> {
    review: &'a str,
    event: Verdict,
    body: &'a str,
}

pub fn submit_body(review_id: &str, verdict: Verdict, summary: &str) -> String {
    body(
        SUBMIT,
        SubmitVariables {
            review: review_id,
            event: verdict,
            body: summary,
        },
    )
}

#[derive(Serialize)]
struct DeleteVariables<'a> {
    review: &'a str,
}

pub fn delete_body(review_id: &str) -> String {
    body(DELETE, DeleteVariables { review: review_id })
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
    add_pull_request_review: Option<ReviewPayload>,
    add_pull_request_review_thread: Option<ThreadPayload>,
    submit_pull_request_review: Option<ReviewPayload>,
    delete_pull_request_review: Option<ReviewPayload>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewPayload {
    pull_request_review: Option<Node>,
}

#[derive(Deserialize)]
struct ThreadPayload {
    thread: Option<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
}

/// The node id a mutation response returns: the review's for create, submit and delete, the
/// thread's for a thread.
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
    let node = data
        .add_pull_request_review
        .or(data.submit_pull_request_review)
        .or(data.delete_pull_request_review)
        .and_then(|p| p.pull_request_review)
        .or_else(|| data.add_pull_request_review_thread.and_then(|p| p.thread))
        .ok_or_else(|| GithubError::Decode("no node in the mutation payload".into()))?;
    Ok(node.id)
}

async fn mutate(
    client: &reqwest::Client,
    token: &str,
    body: String,
) -> Result<String, GithubError> {
    let response = client
        .post(ENDPOINT)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, "prodgy")
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

/// Runs the action step by step, stopping at the first failure but reporting the progress made.
pub async fn run(client: &reqwest::Client, token: &str, action: ReviewAction) -> ReviewResult {
    match action {
        ReviewAction::Discard { review_id } => {
            let outcome = mutate(client, token, delete_body(&review_id))
                .await
                .map(|_| ReviewOutcome::Discarded);
            ReviewResult {
                review_id: Some(review_id),
                added: 0,
                outcome,
            }
        }
        ReviewAction::Submit {
            pull_id,
            head_oid,
            review_id,
            threads,
            verdict,
            body: summary,
        } => {
            let mut result = ReviewResult {
                review_id,
                added: 0,
                outcome: Ok(ReviewOutcome::Submitted),
            };
            let review_id = match result.review_id.clone() {
                Some(id) => id,
                None => match mutate(client, token, create_body(&pull_id, &head_oid)).await {
                    Ok(id) => {
                        result.review_id = Some(id.clone());
                        id
                    }
                    Err(e) => {
                        result.outcome = Err(e);
                        return result;
                    }
                },
            };
            for thread in &threads {
                if let Err(e) = mutate(client, token, thread_body(&review_id, thread)).await {
                    result.outcome = Err(e);
                    return result;
                }
                result.added += 1;
            }
            if let Err(e) = mutate(client, token, submit_body(&review_id, verdict, &summary)).await
            {
                result.outcome = Err(e);
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(body: &str) -> serde_json::Value {
        serde_json::from_str(body).expect("json")
    }

    #[test]
    /// GH-R-021, GH-E-013 — the four mutations carry their inputs; a single-line thread omits the start fields and a range sends them with the same side; the verdicts and sides serialize as the API's enums.
    fn ut_request_bodies() {
        let create = json(&create_body("PR_1", "abc"));
        assert!(
            create["query"].as_str().expect("q").contains(
                "addPullRequestReview(input: {pullRequestId: $pull, commitOID: $commit})"
            )
        );
        assert_eq!(create["variables"]["pull"], "PR_1");
        assert_eq!(create["variables"]["commit"], "abc");

        let single = json(&thread_body(
            "RV_1",
            &Thread {
                path: "src/main.rs".into(),
                side: Side::Right,
                start_line: 12,
                line: 12,
                body: "nit".into(),
            },
        ));
        assert!(
            single["query"]
                .as_str()
                .expect("q")
                .contains("addPullRequestReviewThread")
        );
        assert_eq!(single["variables"]["review"], "RV_1");
        assert_eq!(single["variables"]["path"], "src/main.rs");
        assert_eq!(single["variables"]["line"], 12);
        assert_eq!(single["variables"]["side"], "RIGHT");
        assert_eq!(single["variables"]["body"], "nit");
        assert!(single["variables"].get("startLine").is_none(), "{single}");
        assert!(single["variables"].get("startSide").is_none(), "{single}");

        let range = json(&thread_body(
            "RV_1",
            &Thread {
                path: "a".into(),
                side: Side::Left,
                start_line: 3,
                line: 5,
                body: "b".into(),
            },
        ));
        assert_eq!(range["variables"]["startLine"], 3);
        assert_eq!(range["variables"]["line"], 5);
        assert_eq!(range["variables"]["startSide"], "LEFT");
        assert_eq!(range["variables"]["side"], "LEFT");

        let submit = json(&submit_body("RV_1", Verdict::RequestChanges, "please"));
        assert!(
            submit["query"]
                .as_str()
                .expect("q")
                .contains("submitPullRequestReview")
        );
        assert_eq!(submit["variables"]["event"], "REQUEST_CHANGES");
        assert_eq!(submit["variables"]["body"], "please");
        assert_eq!(
            json(&submit_body("r", Verdict::Approve, ""))["variables"]["event"],
            "APPROVE"
        );
        assert_eq!(
            json(&submit_body("r", Verdict::Comment, ""))["variables"]["event"],
            "COMMENT"
        );

        let delete = json(&delete_body("RV_1"));
        assert!(
            delete["query"]
                .as_str()
                .expect("q")
                .contains("deletePullRequestReview")
        );
        assert_eq!(delete["variables"]["review"], "RV_1");
    }

    #[test]
    /// GH-R-021 — every mutation payload yields its node id; a GraphQL error carries its message; a payload without a node and garbage are decode errors.
    fn ut_parse_id() {
        assert_eq!(
            parse_id(r#"{"data":{"addPullRequestReview":{"pullRequestReview":{"id":"RV_1"}}}}"#)
                .expect("id"),
            "RV_1"
        );
        assert_eq!(
            parse_id(r#"{"data":{"addPullRequestReviewThread":{"thread":{"id":"TH_1"}}}}"#)
                .expect("id"),
            "TH_1"
        );
        assert_eq!(
            parse_id(r#"{"data":{"submitPullRequestReview":{"pullRequestReview":{"id":"RV_1"}}}}"#)
                .expect("id"),
            "RV_1"
        );
        assert_eq!(
            parse_id(r#"{"data":{"deletePullRequestReview":{"pullRequestReview":{"id":"RV_1"}}}}"#)
                .expect("id"),
            "RV_1"
        );
        assert!(matches!(
            parse_id(r#"{"data":null,"errors":[{"message":"Resource not accessible"}]}"#),
            Err(GithubError::Graphql(m)) if m == "Resource not accessible"
        ));
        assert!(matches!(
            parse_id(r#"{"data":{"addPullRequestReview":null}}"#),
            Err(GithubError::Decode(_))
        ));
        assert!(matches!(parse_id("{"), Err(GithubError::Decode(_))));
        let body = r#"{"data":{"addPullRequestReview":{"pullRequestReview":{"id":"RV_1"}}}}"#;
        for end in 0..body.len() {
            let _ = parse_id(&body[..end]);
        }
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
    /// GH-R-021, GH-E-014 — an unreachable host is a typed HTTP error; a submit that fails at its first step reports no review id and no thread added, one continuing a held review keeps that id; a discard reports the id it tried to delete.
    async fn ut_run_reports_progress_on_failure() {
        let client = unreachable_client();
        let fresh = run(
            &client,
            "t",
            ReviewAction::Submit {
                pull_id: "PR_1".into(),
                head_oid: "abc".into(),
                review_id: None,
                threads: vec![],
                verdict: Verdict::Approve,
                body: String::new(),
            },
        )
        .await;
        assert!(
            matches!(fresh.outcome, Err(GithubError::Http(_))),
            "{:?}",
            fresh.outcome
        );
        assert_eq!((fresh.review_id, fresh.added), (None, 0));
        let held = run(
            &client,
            "t",
            ReviewAction::Submit {
                pull_id: "PR_1".into(),
                head_oid: "abc".into(),
                review_id: Some("RV_9".into()),
                threads: vec![Thread {
                    path: "a".into(),
                    side: Side::Right,
                    start_line: 1,
                    line: 1,
                    body: "b".into(),
                }],
                verdict: Verdict::Comment,
                body: String::new(),
            },
        )
        .await;
        assert!(matches!(held.outcome, Err(GithubError::Http(_))));
        assert_eq!((held.review_id.as_deref(), held.added), (Some("RV_9"), 0));
        let discard = run(
            &client,
            "t",
            ReviewAction::Discard {
                review_id: "RV_9".into(),
            },
        )
        .await;
        assert!(matches!(discard.outcome, Err(GithubError::Http(_))));
        assert_eq!(discard.review_id.as_deref(), Some("RV_9"));
    }
}
