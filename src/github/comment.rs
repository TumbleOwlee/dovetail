//! Posting a conversation comment on a pull request or an issue.

use serde::{Deserialize, Serialize};

use crate::github::GithubError;

const ENDPOINT: &str = "https://api.github.com/graphql";

const ADD: &str = "mutation($subject: ID!, $body: String!) { addComment(input: {subjectId: $subject, body: $body}) { commentEdge { node { id } } } }";

#[derive(Serialize)]
struct Request<'a, V: Serialize> {
    query: &'a str,
    variables: V,
}

#[derive(Serialize)]
struct AddVariables<'a> {
    subject: &'a str,
    body: &'a str,
}

pub fn add_body(subject_id: &str, body: &str) -> String {
    serde_json::to_string(&Request {
        query: ADD,
        variables: AddVariables {
            subject: subject_id,
            body,
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
#[serde(rename_all = "camelCase")]
struct Data {
    add_comment: Option<Payload>,
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
struct Node {
    id: String,
}

/// The posted comment's node id out of the mutation response.
pub fn parse_id(body: &str) -> Result<String, GithubError> {
    let response: Response =
        serde_json::from_str(body).map_err(|e| GithubError::Decode(e.to_string()))?;
    if let Some(first) = response
        .errors
        .and_then(|mut errors| errors.drain(..).next())
    {
        return Err(GithubError::Graphql(first.message));
    }
    response
        .data
        .and_then(|d| d.add_comment)
        .and_then(|p| p.comment_edge)
        .and_then(|e| e.node)
        .map(|n| n.id)
        .ok_or_else(|| GithubError::Decode("no node in the mutation payload".into()))
}

/// Posts the comment; the caller refetches the details on success.
pub async fn post(
    client: &reqwest::Client,
    token: &str,
    subject_id: &str,
    body: &str,
) -> Result<String, GithubError> {
    let response = client
        .post(ENDPOINT)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, "dovetail")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(add_body(subject_id, body))
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(GithubError::Status(status.as_u16()));
    }
    parse_id(&response.text().await?)
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
    /// GH-R-024, GH-E-015 — the response parses to the comment's node id; a GraphQL error message becomes the typed error; a missing node is a decode error; truncated input never crashes.
    fn ut_parse_id() {
        let ok = r#"{"data":{"addComment":{"commentEdge":{"node":{"id":"IC_9"}}}}}"#;
        assert_eq!(parse_id(ok).expect("id"), "IC_9");
        let locked = r#"{"data":null,"errors":[{"message":"Unlocked conversations only"}]}"#;
        assert!(matches!(
            parse_id(locked),
            Err(GithubError::Graphql(m)) if m == "Unlocked conversations only"
        ));
        let empty = r#"{"data":{"addComment":null}}"#;
        assert!(matches!(parse_id(empty), Err(GithubError::Decode(_))));
        assert!(matches!(
            parse_id(r#"{"data":{"addComm"#),
            Err(GithubError::Decode(_))
        ));
    }
}
