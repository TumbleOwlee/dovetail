//! One pull request's details and timeline.

use serde::{Deserialize, Serialize};

use super::board::Label;
use super::files::{self, ChangedFile};
use super::projects::GithubError;
use super::pulls::PullState;
use super::review;
use super::timeline::{self, TimelineItem};

const ENDPOINT: &str = "https://api.github.com/graphql";

const QUERY_HEAD: &str = "query($owner: String!, $name: String!, $number: Int!, $after: String) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { id number title body state isDraft url headRefOid repository { nameWithOwner } author { login } reviewRequests(first: 20) { nodes { requestedReviewer { __typename ... on User { login } ... on Team { name } } } } latestReviews(first: 20) { nodes { state author { login } } } assignees(first: 10) { nodes { login } } labels(first: 20) { nodes { name color } } projectItems(first: 10) { nodes { project { title } } } milestone { title } closingIssuesReferences(first: 10) { nodes { id number title } } participants(first: 20) { nodes { login } } commits(first: 100) { nodes { commit { abbreviatedOid messageHeadline committedDate author { name user { login } } } } } reviewThreads(first: 100) { nodes { id isResolved isOutdated path diffSide line startLine originalLine originalStartLine comments(first: 100) { nodes { author { login } body } } } }";

const QUERY_TAIL: &str = " } } }";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    Pending,
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reviewer {
    /// A user login or a team name.
    pub name: String,
    pub state: ReviewState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PullDetails {
    /// GraphQL node id, the handle for mutations.
    pub id: String,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: PullState,
    pub draft: bool,
    pub url: String,
    /// The head commit's object id.
    pub head_oid: String,
    /// `owner/name` of the repository the pull request lives in.
    pub repository: String,
    pub author: Option<String>,
    /// Requested reviewers first, then those with a latest review.
    pub reviewers: Vec<Reviewer>,
    pub assignees: Vec<String>,
    pub labels: Vec<Label>,
    pub projects: Vec<String>,
    pub milestone: Option<String>,
    pub development: Vec<IssueRef>,
    pub participants: Vec<String>,
    /// The first 100 commits, oldest first as GitHub lists them.
    pub commits: Vec<Commit>,
    /// Filled by `load_pull_request` from the REST files endpoint; empty in a parsed page.
    pub files: Vec<ChangedFile>,
    /// Review comment threads on the diff, resolved ones included.
    pub threads: Vec<ReviewThread>,
    pub timeline: Vec<TimelineItem>,
}

/// One comment of a review thread; `author` is `None` when the account was deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadComment {
    pub author: Option<String>,
    pub body: String,
}

/// One review comment thread anchored to a file line range of the pull request diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewThread {
    /// GraphQL node id, the handle for replies.
    pub id: String,
    pub path: String,
    pub side: review::Side,
    /// First line of the range; equal to `line` for a single line.
    pub start_line: u32,
    /// Last line, or the only one.
    pub line: u32,
    pub resolved: bool,
    /// The diff moved on; the lines are the original anchor.
    pub outdated: bool,
    pub comments: Vec<ThreadComment>,
}

/// Who authored a commit: the GitHub account it is linked to, or only the git author name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitAuthor {
    User(String),
    Git(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Abbreviated object id.
    pub sha: String,
    pub headline: String,
    pub author: CommitAuthor,
    /// Committed date, ISO 8601.
    pub date: String,
}

/// One page of a details response: the details with this page's timeline items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub details: PullDetails,
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
    owner: &'a str,
    name: &'a str,
    number: u64,
    after: Option<&'a str>,
}

/// The query with the pull request timeline selection in place.
fn query() -> String {
    format!(
        "{QUERY_HEAD} {} {QUERY_TAIL}",
        timeline::selection(timeline::Owner::Pull)
    )
}

/// The JSON body sent to the GraphQL endpoint.
pub fn request_body(owner: &str, repo: &str, number: u64, after: Option<&str>) -> String {
    serde_json::to_string(&Request {
        query: &query(),
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
struct RepoName {
    name_with_owner: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    id: String,
    number: u64,
    title: String,
    body: String,
    state: PullState,
    is_draft: bool,
    url: String,
    head_ref_oid: String,
    repository: RepoName,
    author: Option<Author>,
    review_requests: Nodes<ReviewRequest>,
    latest_reviews: Nodes<Review>,
    assignees: Nodes<Author>,
    labels: Nodes<LabelNode>,
    project_items: Nodes<ProjectItem>,
    milestone: Option<Milestone>,
    closing_issues_references: Nodes<IssueRef>,
    participants: Nodes<Author>,
    commits: Nodes<CommitNode>,
    review_threads: Nodes<ThreadNode>,
    timeline_items: timeline::Connection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadNode {
    id: String,
    is_resolved: bool,
    is_outdated: bool,
    path: String,
    diff_side: review::Side,
    line: Option<u32>,
    start_line: Option<u32>,
    original_line: Option<u32>,
    original_start_line: Option<u32>,
    comments: Nodes<CommentNode>,
}

#[derive(Deserialize)]
struct CommentNode {
    author: Option<Author>,
    body: String,
}

/// The threads with an anchor line: `line` falls back to `originalLine` for an outdated
/// thread, `startLine` to `originalStartLine` to `line`; a thread with neither is dropped.
fn review_threads(nodes: Vec<ThreadNode>) -> Vec<ReviewThread> {
    nodes
        .into_iter()
        .filter_map(|t| {
            let line = t.line.or(t.original_line)?;
            let start_line = t.start_line.or(t.original_start_line).unwrap_or(line);
            Some(ReviewThread {
                id: t.id,
                path: t.path,
                side: t.diff_side,
                start_line,
                line,
                resolved: t.is_resolved,
                outdated: t.is_outdated,
                comments: t
                    .comments
                    .nodes
                    .into_iter()
                    .map(|c| ThreadComment {
                        author: c.author.map(|a| a.login),
                        body: c.body,
                    })
                    .collect(),
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct CommitNode {
    commit: CommitData,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitData {
    abbreviated_oid: String,
    message_headline: String,
    committed_date: String,
    author: Option<GitAuthor>,
}

#[derive(Deserialize)]
struct GitAuthor {
    name: Option<String>,
    user: Option<Author>,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewRequest {
    requested_reviewer: Option<RequestedReviewer>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum RequestedReviewer {
    User {
        login: String,
    },
    Team {
        name: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Review {
    state: ReviewState,
    author: Option<Author>,
}

#[derive(Deserialize)]
struct LabelNode {
    name: String,
    color: String,
}

#[derive(Deserialize)]
struct ProjectItem {
    project: ProjectTitle,
}

#[derive(Deserialize)]
struct ProjectTitle {
    title: String,
}

#[derive(Deserialize)]
struct Milestone {
    title: String,
}

/// An issue a pull request closes; `id` is the GraphQL node id the issue is loaded by.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IssueRef {
    pub id: String,
    pub number: u64,
    pub title: String,
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
    let next_cursor = node.timeline_items.next_cursor();
    let timeline = node.timeline_items.items();
    let mut reviewers: Vec<Reviewer> = node
        .review_requests
        .nodes
        .into_iter()
        .filter_map(|r| match r.requested_reviewer? {
            RequestedReviewer::User { login } => Some(login),
            RequestedReviewer::Team { name } => Some(name),
            RequestedReviewer::Other => None,
        })
        .map(|name| Reviewer {
            name,
            state: ReviewState::Pending,
        })
        .collect();
    reviewers.extend(node.latest_reviews.nodes.into_iter().map(|r| Reviewer {
        name: r.author.map_or_else(|| "ghost".to_string(), |a| a.login),
        state: r.state,
    }));
    Ok(Page {
        details: PullDetails {
            id: node.id,
            number: node.number,
            title: node.title,
            body: node.body,
            state: node.state,
            draft: node.is_draft,
            url: node.url,
            head_oid: node.head_ref_oid,
            repository: node.repository.name_with_owner,
            author: node.author.map(|a| a.login),
            reviewers,
            assignees: node.assignees.nodes.into_iter().map(|a| a.login).collect(),
            labels: node
                .labels
                .nodes
                .into_iter()
                .map(|l| Label {
                    name: l.name,
                    color: l.color,
                })
                .collect(),
            projects: node
                .project_items
                .nodes
                .into_iter()
                .map(|p| p.project.title)
                .collect(),
            milestone: node.milestone.map(|m| m.title),
            development: node.closing_issues_references.nodes,
            participants: node
                .participants
                .nodes
                .into_iter()
                .map(|a| a.login)
                .collect(),
            commits: node
                .commits
                .nodes
                .into_iter()
                .map(|c| {
                    let c = c.commit;
                    let author = match c.author {
                        Some(GitAuthor {
                            user: Some(user), ..
                        }) => CommitAuthor::User(user.login),
                        Some(GitAuthor { name, .. }) => CommitAuthor::Git(name.unwrap_or_default()),
                        None => CommitAuthor::Git(String::new()),
                    };
                    Commit {
                        sha: c.abbreviated_oid,
                        headline: c.message_headline,
                        author,
                        date: c.committed_date,
                    }
                })
                .collect(),
            files: Vec::new(),
            threads: review_threads(node.review_threads.nodes),
            timeline,
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
            Some(details) => details.timeline.extend(page.details.timeline),
            None => details = Some(page.details),
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            let mut details = details.expect("the first page was stored above");
            details.files = files::load_changed_files(client, token, owner, repo, number).await?;
            return Ok(details);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"data":{"repository":{"pullRequest":{"id":"PR_5","number":5,"title":"Fix crash","body":"Fixes #7","state":"OPEN","isDraft":false,"url":"https://github.com/o/r/pull/5","headRefOid":"0123abcd","repository":{"nameWithOwner":"o/r"},"author":{"login":"octo"},
        "reviewRequests":{"nodes":[{"requestedReviewer":{"__typename":"User","login":"rev"}},{"requestedReviewer":{"__typename":"Team","name":"core"}},{"requestedReviewer":null}]},
        "latestReviews":{"nodes":[{"state":"APPROVED","author":{"login":"a"}},{"state":"CHANGES_REQUESTED","author":null}]},
        "assignees":{"nodes":[{"login":"b"}]},"labels":{"nodes":[{"name":"bug","color":"d73a4a"}]},
        "projectItems":{"nodes":[{"project":{"title":"Roadmap"}}]},"milestone":{"title":"v1"},
        "closingIssuesReferences":{"nodes":[{"id":"I_7","number":7,"title":"Crash on start"}]},"participants":{"nodes":[{"login":"octo"},{"login":"a"}]},
        "commits":{"nodes":[{"commit":{"abbreviatedOid":"abc1234","messageHeadline":"Fix crash","committedDate":"2026-09-03T10:00:00Z","author":{"name":"Octo Cat","user":{"login":"octo"}}}},{"commit":{"abbreviatedOid":"def5678","messageHeadline":"Add test","committedDate":"2026-09-04T10:00:00Z","author":{"name":"Anon","user":null}}}]},
        "reviewThreads":{"nodes":[
        {"id":"TH_1","isResolved":false,"isOutdated":false,"path":"src/main.rs","diffSide":"RIGHT","line":4,"startLine":2,"originalLine":null,"originalStartLine":null,"comments":{"nodes":[{"author":{"login":"a"},"body":"why?"},{"author":null,"body":"gone"}]}},
        {"id":"TH_2","isResolved":true,"isOutdated":false,"path":"src/main.rs","diffSide":"LEFT","line":9,"startLine":null,"originalLine":null,"originalStartLine":null,"comments":{"nodes":[{"author":{"login":"b"},"body":"done"}]}},
        {"id":"TH_3","isResolved":false,"isOutdated":true,"path":"old.rs","diffSide":"RIGHT","line":null,"startLine":null,"originalLine":7,"originalStartLine":6,"comments":{"nodes":[]}},
        {"id":"TH_4","isResolved":false,"isOutdated":true,"path":"gone.rs","diffSide":"RIGHT","line":null,"startLine":null,"originalLine":null,"originalStartLine":null,"comments":{"nodes":[]}}]},
        "timelineItems":{"pageInfo":{"hasNextPage":true,"endCursor":"cur"},"nodes":[
        {"__typename":"IssueComment","body":"LGTM","createdAt":"2026-09-04T10:00:00Z","author":{"login":"a"}},
        {"__typename":"MergedEvent","actor":null,"createdAt":"2026-09-05T10:00:00Z"}
    ]}}}}}"#;

    #[test]
    /// GH-R-015, GH-R-018 — the query asks for the pull request by number with its commits and its timeline page including merge and review events.
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
            "id number title body state isDraft url headRefOid repository { nameWithOwner } author { login }",
            "reviewRequests(first: 20) { nodes { requestedReviewer { __typename ... on User { login } ... on Team { name } } } }",
            "latestReviews(first: 20) { nodes { state author { login } } }",
            "assignees(first: 10) { nodes { login } }",
            "labels(first: 20) { nodes { name color } }",
            "projectItems(first: 10) { nodes { project { title } } }",
            "milestone { title }",
            "closingIssuesReferences(first: 10) { nodes { id number title } }",
            "participants(first: 20) { nodes { login } }",
            "commits(first: 100) { nodes { commit { abbreviatedOid messageHeadline committedDate author { name user { login } } } } }",
            "reviewThreads(first: 100) { nodes { id isResolved isOutdated path diffSide line startLine originalLine originalStartLine comments(first: 100) { nodes { author { login } body } } } }",
            "timelineItems(first: 100, after: $after, itemTypes: [ISSUE_COMMENT, ",
            "MERGED_EVENT, REVIEW_REQUESTED_EVENT, PULL_REQUEST_REVIEW,",
            "... on MergedEvent { actor { login } createdAt }",
            "pageInfo { hasNextPage endCursor }",
        ] {
            assert!(query.contains(part), "{part} missing in {query}");
        }
    }

    #[test]
    /// GH-R-015, GH-R-017, GH-R-018 — details, commits and timeline items are carried, deleted authors are `None`, the cursor follows `hasNextPage`.
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
        assert_eq!(d.head_oid, "0123abcd");
        assert_eq!(d.id, "PR_5");
        assert_eq!(d.repository, "o/r");
        assert_eq!(d.author.as_deref(), Some("octo"));
        assert_eq!(d.timeline.len(), 2);
        assert_eq!(d.timeline[0].actor.as_deref(), Some("a"));
        assert_eq!(
            d.timeline[0].event,
            timeline::Event::Comment {
                body: "LGTM".into()
            }
        );
        assert_eq!(d.timeline[1].actor, None);
        assert_eq!(d.timeline[1].event, timeline::Event::Merged);
        let last = BODY.replace(r#""hasNextPage":true"#, r#""hasNextPage":false"#);
        assert_eq!(parse_page(&last).expect("parses").next_cursor, None);
    }

    #[test]
    /// GH-R-015 — sidebar fields: requested users and teams as pending, latest reviews with state, a deleted reviewer as ghost; a missing milestone is `None`.
    fn ut_parse_sidebar_fields() {
        let d = parse_page(BODY).expect("parses").details;
        let reviewers: Vec<(&str, ReviewState)> = d
            .reviewers
            .iter()
            .map(|r| (r.name.as_str(), r.state))
            .collect();
        assert_eq!(
            reviewers,
            vec![
                ("rev", ReviewState::Pending),
                ("core", ReviewState::Pending),
                ("a", ReviewState::Approved),
                ("ghost", ReviewState::ChangesRequested),
            ]
        );
        assert_eq!(d.assignees, vec!["b".to_string()]);
        assert_eq!(d.labels[0].name, "bug");
        assert_eq!(d.projects, vec!["Roadmap".to_string()]);
        assert_eq!(d.milestone.as_deref(), Some("v1"));
        assert_eq!(
            d.development,
            vec![IssueRef {
                id: "I_7".into(),
                number: 7,
                title: "Crash on start".into()
            }]
        );
        assert_eq!(d.participants, vec!["octo".to_string(), "a".to_string()]);
        assert_eq!(
            d.commits,
            vec![
                Commit {
                    sha: "abc1234".into(),
                    headline: "Fix crash".into(),
                    author: CommitAuthor::User("octo".into()),
                    date: "2026-09-03T10:00:00Z".into()
                },
                Commit {
                    sha: "def5678".into(),
                    headline: "Add test".into(),
                    author: CommitAuthor::Git("Anon".into()),
                    date: "2026-09-04T10:00:00Z".into()
                }
            ]
        );
        let bare = BODY.replace(r#""milestone":{"title":"v1"}"#, r#""milestone":null"#);
        assert_eq!(parse_page(&bare).expect("parses").details.milestone, None);
    }

    #[test]
    /// GH-R-023 — review threads carry their id, path, side, resolved and outdated flags and comments with a deleted author as `None`; an outdated thread's lines fall back to the original anchor; a missing `startLine` equals `line`; a thread with no line at all is dropped.
    fn ut_parse_review_threads() {
        let threads = parse_page(BODY).expect("parses").details.threads;
        assert_eq!(threads.len(), 3, "the unanchored thread is dropped");
        assert_eq!(
            threads[0],
            ReviewThread {
                id: "TH_1".into(),
                path: "src/main.rs".into(),
                side: review::Side::Right,
                start_line: 2,
                line: 4,
                resolved: false,
                outdated: false,
                comments: vec![
                    ThreadComment {
                        author: Some("a".into()),
                        body: "why?".into()
                    },
                    ThreadComment {
                        author: None,
                        body: "gone".into()
                    },
                ],
            }
        );
        assert_eq!(
            (
                threads[1].resolved,
                threads[1].side,
                threads[1].start_line,
                threads[1].line
            ),
            (true, review::Side::Left, 9, 9),
            "startLine falls back to line"
        );
        assert_eq!(
            (threads[2].outdated, threads[2].start_line, threads[2].line),
            (true, 6, 7),
            "outdated lines fall back to the original anchor"
        );
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
