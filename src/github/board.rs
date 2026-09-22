//! Loading a Projects (v2) board: its `Status` columns and the issues on it.

use serde::{Deserialize, Serialize};

use super::projects::GithubError;

const ENDPOINT: &str = "https://api.github.com/graphql";
const QUERY: &str = "query($login: String!, $number: Int!, $after: String) { repositoryOwner(login: $login) { ... on ProjectV2Owner { projectV2(number: $number) { title field(name: \"Status\") { ... on ProjectV2SingleSelectField { options { name } } } items(first: 100, after: $after) { pageInfo { hasNextPage endCursor } nodes { fieldValueByName(name: \"Status\") { ... on ProjectV2ItemFieldSingleSelectValue { name } } content { __typename ... on Issue { id title number labels(first: 10) { nodes { name color } } assignees(first: 5) { nodes { login } } } } } } } } } }";

/// A column of the board, named by a `Status` option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub name: String,
    /// Six hex digits as GitHub sends them, e.g. `d73a4a`.
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// GraphQL node id, the key for the details request.
    pub id: String,
    pub number: u64,
    pub title: String,
    pub labels: Vec<Label>,
    pub assignees: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub title: String,
    pub columns: Vec<Column>,
}

#[derive(Serialize)]
struct Request<'a> {
    query: &'a str,
    variables: Variables<'a>,
}

#[derive(Serialize)]
struct Variables<'a> {
    login: &'a str,
    number: u64,
    after: Option<&'a str>,
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
    project_v2: Option<Project>,
}

#[derive(Deserialize)]
struct Project {
    title: String,
    field: Option<Field>,
    items: Items,
}

#[derive(Deserialize)]
struct Field {
    #[serde(default)]
    options: Vec<Option_>,
}

#[derive(Deserialize)]
struct Option_ {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Items {
    #[serde(default)]
    page_info: Option<PageInfo>,
    nodes: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    field_value_by_name: Option<StatusValue>,
    content: Option<Content>,
}

#[derive(Deserialize)]
struct StatusValue {
    name: Option<String>,
}

/// Only issues carry fields the board uses; other content types deserialize to `Other`.
#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Content {
    Issue {
        id: String,
        title: String,
        number: u64,
        labels: Nodes<LabelNode>,
        assignees: Nodes<Assignee>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
pub(super) struct Nodes<T> {
    pub(super) nodes: Vec<T>,
}

#[derive(Deserialize)]
pub(super) struct LabelNode {
    pub(super) name: String,
    pub(super) color: String,
}

#[derive(Deserialize)]
pub(super) struct Assignee {
    pub(super) login: String,
}

/// The JSON body sent to the GraphQL endpoint.
pub fn request_body(owner: &str, number: u64, after: Option<&str>) -> String {
    serde_json::to_string(&Request {
        query: QUERY,
        variables: Variables {
            login: owner,
            number,
            after,
        },
    })
    .expect("a request of plain values serializes")
}

/// One page of a board response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub board: Board,
    /// Cursor of the next items page, `None` on the last page.
    pub next_cursor: Option<String>,
}

impl Board {
    /// Append `more`'s cards column by column.
    pub fn extend(&mut self, more: Board) {
        for (column, extra) in self.columns.iter_mut().zip(more.columns) {
            column.cards.extend(extra.cards);
        }
    }
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
    let owner = response
        .data
        .and_then(|d| d.repository_owner)
        .ok_or(GithubError::MissingOwner)?;
    let project = owner.project_v2.ok_or(GithubError::MissingProject)?;
    let mut columns: Vec<Column> = project
        .field
        .into_iter()
        .flat_map(|f| f.options)
        .map(|o| Column {
            name: o.name,
            cards: Vec::new(),
        })
        .collect();
    columns.push(Column {
        name: "No status".to_string(),
        cards: Vec::new(),
    });
    let last = columns.len() - 1;
    let next_cursor = project
        .items
        .page_info
        .filter(|p| p.has_next_page)
        .and_then(|p| p.end_cursor);
    for item in project.items.nodes {
        let Some(Content::Issue {
            id,
            title,
            number,
            labels,
            assignees,
        }) = item.content
        else {
            continue;
        };
        let status = item.field_value_by_name.and_then(|v| v.name);
        let index = status
            .and_then(|name| columns[..last].iter().position(|c| c.name == name))
            .unwrap_or(last);
        columns[index].cards.push(Card {
            id,
            number,
            title,
            labels: labels
                .nodes
                .into_iter()
                .map(|l| Label {
                    name: l.name,
                    color: l.color,
                })
                .collect(),
            assignees: assignees.nodes.into_iter().map(|a| a.login).collect(),
        });
    }
    Ok(Page {
        board: Board {
            title: project.title,
            columns,
        },
        next_cursor,
    })
}

/// The board from a single-page GraphQL response body.
#[cfg(test)]
pub fn parse_board(body: &str) -> Result<Board, GithubError> {
    parse_page(body).map(|p| p.board)
}

pub async fn load_board(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    number: u64,
) -> Result<Board, GithubError> {
    let mut board: Option<Board> = None;
    let mut cursor: Option<String> = None;
    loop {
        let response = client
            .post(ENDPOINT)
            .bearer_auth(token)
            .header(reqwest::header::USER_AGENT, "dovetail")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request_body(owner, number, cursor.as_deref()))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(GithubError::Status(status.as_u16()));
        }
        let page = parse_page(&response.text().await?)?;
        match board.as_mut() {
            Some(board) => board.extend(page.board),
            None => board = Some(page.board),
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(board.expect("the first page was stored above"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"data":{"repositoryOwner":{"projectV2":{"title":"Roadmap","field":{"options":[{"name":"Todo"},{"name":"In Progress"},{"name":"Done"}]},"items":{"nodes":[
        {"fieldValueByName":{"name":"Todo"},"content":{"__typename":"Issue","id":"I_1","title":"First","number":1,"labels":{"nodes":[{"name":"bug","color":"d73a4a"}]},"assignees":{"nodes":[{"login":"octo"}]}}},
        {"fieldValueByName":null,"content":{"__typename":"Issue","id":"I_2","title":"Loose","number":2,"labels":{"nodes":[]},"assignees":{"nodes":[]}}},
        {"fieldValueByName":{"name":"Done"},"content":{"__typename":"PullRequest","title":"PR","number":3}},
        {"fieldValueByName":{"name":"Done"},"content":{"__typename":"DraftIssue","title":"Draft"}},
        {"fieldValueByName":{"name":"Gone"},"content":{"__typename":"Issue","id":"I_4","title":"Orphan","number":4,"labels":{"nodes":[]},"assignees":{"nodes":[{"login":"a"},{"login":"b"}]}}},
        {"fieldValueByName":null,"content":null}
    ]}}}}}"#;

    #[test]
    /// GH-R-004 — the query asks for the owner's project by number with its Status field and 100 items.
    fn ut_request_body_shape() {
        let body: serde_json::Value =
            serde_json::from_str(&request_body("octo", 7, None)).expect("json");
        let query = body["query"].as_str().expect("query");
        for needle in [
            "projectV2(number: $number)",
            "field(name: \"Status\")",
            "items(first: 100, after: $after)",
            "... on Issue { id title number",
            "pageInfo { hasNextPage endCursor }",
            "... on Issue",
        ] {
            assert!(query.contains(needle), "{needle} missing in {query}");
        }
        assert_eq!(body["variables"]["login"], "octo");
        assert_eq!(body["variables"]["number"], 7);
    }

    #[test]
    /// GH-R-005, GH-R-007 — columns follow the option order plus `No status`; items land by status name.
    fn ut_columns_follow_options_then_no_status() {
        let board = parse_board(BODY).expect("parses");
        assert_eq!(board.title, "Roadmap");
        let names: Vec<&str> = board.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Todo", "In Progress", "Done", "No status"]);
        assert_eq!(board.columns[0].cards.len(), 1);
        assert_eq!(board.columns[1].cards.len(), 0);
        assert_eq!(board.columns[2].cards.len(), 0);
        let loose: Vec<u64> = board.columns[3].cards.iter().map(|c| c.number).collect();
        assert_eq!(loose, vec![2, 4]);
    }

    #[test]
    /// GH-R-006, GH-R-008 — issues become cards with labels and assignees; other content is skipped.
    fn ut_issue_cards_carry_labels_and_assignees() {
        let board = parse_board(BODY).expect("parses");
        assert_eq!(
            board.columns[0].cards[0],
            Card {
                id: "I_1".into(),
                number: 1,
                title: "First".into(),
                labels: vec![Label {
                    name: "bug".into(),
                    color: "d73a4a".into()
                }],
                assignees: vec!["octo".into()],
            }
        );
        assert_eq!(board.columns[3].cards[1].assignees, vec!["a", "b"]);
        let total: usize = board.columns.iter().map(|c| c.cards.len()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    /// GH-E-003 — no Status field: a single `No status` column holds everything.
    fn ut_missing_status_field_yields_no_status_only() {
        let body = r#"{"data":{"repositoryOwner":{"projectV2":{"title":"T","field":null,"items":{"nodes":[{"fieldValueByName":null,"content":{"__typename":"Issue","id":"I_9","title":"X","number":9,"labels":{"nodes":[]},"assignees":{"nodes":[]}}}]}}}}}"#;
        let board = parse_board(body).expect("parses");
        assert_eq!(board.columns.len(), 1);
        assert_eq!(board.columns[0].name, "No status");
        assert_eq!(board.columns[0].cards[0].number, 9);
    }

    #[test]
    /// GH-R-009 — a missing project, a GraphQL error and garbage are typed errors.
    fn ut_errors_are_typed() {
        let err =
            parse_board(r#"{"data":{"repositoryOwner":{"projectV2":null}}}"#).expect_err("rejects");
        assert!(matches!(err, GithubError::MissingProject), "{err}");
        let err = parse_board(r#"{"data":{"repositoryOwner":null}}"#).expect_err("rejects");
        assert!(matches!(err, GithubError::MissingOwner), "{err}");
        let err = parse_board(r#"{"errors":[{"message":"nope"}]}"#).expect_err("rejects");
        assert!(
            matches!(err, GithubError::Graphql(ref m) if m == "nope"),
            "{err}"
        );
        assert!(matches!(parse_board("{"), Err(GithubError::Decode(_))));
    }

    #[test]
    /// GH-R-004 — the cursor of the previous page is sent as `after`; a page reports its next cursor; pages merge column-wise.
    fn ut_pages_follow_cursor() {
        let body: serde_json::Value =
            serde_json::from_str(&request_body("octo", 7, Some("abc"))).expect("json");
        assert_eq!(body["variables"]["after"], "abc");
        let none: serde_json::Value =
            serde_json::from_str(&request_body("octo", 7, None)).expect("json");
        assert_eq!(none["variables"]["after"], serde_json::Value::Null);

        let first = BODY.replacen(
            r#""items":{"nodes":["#,
            r#""items":{"pageInfo":{"hasNextPage":true,"endCursor":"cur"},"nodes":["#,
            1,
        );
        let page = parse_page(&first).expect("parses");
        assert_eq!(page.next_cursor.as_deref(), Some("cur"));
        let last = BODY.replacen(
            r#""items":{"nodes":["#,
            r#""items":{"pageInfo":{"hasNextPage":false,"endCursor":"end"},"nodes":["#,
            1,
        );
        let page2 = parse_page(&last).expect("parses");
        assert_eq!(page2.next_cursor, None);
        assert_eq!(parse_page(BODY).expect("parses").next_cursor, None);

        let mut board = page.board;
        board.extend(page2.board);
        assert_eq!(board.columns[0].cards.len(), 2);
        let loose: Vec<u64> = board.columns[3].cards.iter().map(|c| c.number).collect();
        assert_eq!(loose, vec![2, 4, 2, 4]);
    }
}
