//! A file's content at a commit, through the GraphQL `object(expression:)` lookup.

use serde::{Deserialize, Serialize};

use crate::github::GithubError;

const ENDPOINT: &str = "https://api.github.com/graphql";

const QUERY: &str = "query($owner: String!, $name: String!, $expression: String!) { repository(owner: $owner, name: $name) { object(expression: $expression) { ... on Blob { text isBinary } } } }";

/// What the API holds for a path at a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blob {
    Text(String),
    Binary,
    /// The API returned no text for a non-binary blob: it exceeds the size it serves inline.
    TooLarge,
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
    expression: String,
}

/// The request body asking for `path` at commit `oid`.
pub fn request_body(owner: &str, repo: &str, oid: &str, path: &str) -> String {
    serde_json::to_string(&Request {
        query: QUERY,
        variables: Variables {
            owner,
            name: repo,
            expression: format!("{oid}:{path}"),
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
struct Repository {
    object: Option<Object>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Object {
    text: Option<String>,
    is_binary: Option<bool>,
}

/// The blob of a response body: the text, binary, or too large when the text is null.
pub fn parse_blob(body: &str) -> Result<Blob, GithubError> {
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
    let object = repository.object.ok_or(GithubError::MissingBlob)?;
    Ok(match (object.is_binary, object.text) {
        (Some(true), _) => Blob::Binary,
        (_, Some(text)) => Blob::Text(text),
        (_, None) => Blob::TooLarge,
    })
}

pub async fn load_blob(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    repo: &str,
    oid: &str,
    path: &str,
) -> Result<Blob, GithubError> {
    let response = client
        .post(ENDPOINT)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, "dovetail")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request_body(owner, repo, oid, path))
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(GithubError::Status(status.as_u16()));
    }
    parse_blob(&response.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// GH-R-020 — the query asks the repository's object at `<oid>:<path>` for a blob's text and binary flag.
    fn ut_request_body_shape() {
        let body: serde_json::Value =
            serde_json::from_str(&request_body("o", "r", "abc123", "src/main.rs")).expect("json");
        assert_eq!(body["variables"]["owner"], "o");
        assert_eq!(body["variables"]["name"], "r");
        assert_eq!(body["variables"]["expression"], "abc123:src/main.rs");
        let query = body["query"].as_str().expect("query");
        for part in [
            "repository(owner: $owner, name: $name)",
            "object(expression: $expression)",
            "... on Blob { text isBinary }",
        ] {
            assert!(query.contains(part), "{part} missing in {query}");
        }
    }

    #[test]
    /// GH-R-020, GH-E-012 — text, binary and too large are told apart; a null object is `blob not found`; a GraphQL error carries its message; garbage is a decode error.
    fn ut_parse_blob() {
        let wrap = |object: &str| format!(r#"{{"data":{{"repository":{{"object":{object}}}}}}}"#);
        assert_eq!(
            parse_blob(&wrap(r#"{"text":"fn main() {}\n","isBinary":false}"#)).expect("text"),
            Blob::Text("fn main() {}\n".into())
        );
        assert_eq!(
            parse_blob(&wrap(r#"{"text":null,"isBinary":true}"#)).expect("binary"),
            Blob::Binary
        );
        assert_eq!(
            parse_blob(&wrap(r#"{"text":null,"isBinary":false}"#)).expect("too large"),
            Blob::TooLarge
        );
        assert!(matches!(
            parse_blob(&wrap("null")),
            Err(GithubError::MissingBlob)
        ));
        assert_eq!(
            GithubError::MissingBlob.to_string(),
            "github: blob not found"
        );
        assert!(matches!(
            parse_blob(r#"{"data":{"repository":null}}"#),
            Err(GithubError::MissingRepository)
        ));
        assert!(matches!(
            parse_blob(r#"{"data":null,"errors":[{"message":"Bad credentials"}]}"#),
            Err(GithubError::Graphql(m)) if m == "Bad credentials"
        ));
        assert!(matches!(parse_blob("{"), Err(GithubError::Decode(_))));
        let body = wrap(r#"{"text":"abc","isBinary":false}"#);
        for end in 0..body.len() {
            let _ = parse_blob(&body[..end]);
        }
    }

    #[tokio::test]
    /// GH-R-020 — an unreachable host is a typed HTTP error.
    async fn ut_load_unreachable_is_http_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).expect("proxy url"))
            .build()
            .expect("client");
        let result = load_blob(&client, "t", "o", "r", "abc", "p").await;
        assert!(matches!(result, Err(GithubError::Http(_))), "{result:?}");
    }
}
