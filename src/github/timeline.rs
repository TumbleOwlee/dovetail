//! Timeline items shared by issues and pull requests: comments and the events that change them.

use serde::Deserialize;

use super::board::Label;
use super::pull::ReviewState;

/// What one timeline item records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Comment {
        body: String,
    },
    Assigned {
        login: String,
    },
    Unassigned {
        login: String,
    },
    Labeled {
        label: Label,
    },
    Unlabeled {
        label: Label,
    },
    Milestoned {
        title: String,
    },
    Demilestoned {
        title: String,
    },
    Closed {
        reason: Option<String>,
    },
    Reopened,
    Renamed {
        from: String,
        to: String,
    },
    Merged,
    ReviewRequested {
        reviewer: String,
    },
    Reviewed {
        state: ReviewState,
        body: String,
    },
    Referenced {
        id: String,
        headline: String,
    },
    CrossReferenced {
        number: u64,
        title: String,
        repository: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineItem {
    /// `None` when the actor account was deleted.
    pub actor: Option<String>,
    /// ISO 8601 as GitHub sends it.
    pub created_at: String,
    pub event: Event,
}

/// Selection of the `timelineItems` connection: field text after `timelineItems(`, item types
/// common to issues and pull requests.
pub const ISSUE_ITEM_TYPES: &str = "[ISSUE_COMMENT, ASSIGNED_EVENT, UNASSIGNED_EVENT, LABELED_EVENT, UNLABELED_EVENT, MILESTONED_EVENT, DEMILESTONED_EVENT, CLOSED_EVENT, REOPENED_EVENT, RENAMED_TITLE_EVENT, REFERENCED_EVENT, CROSS_REFERENCED_EVENT]";

/// Item types of a pull request: the issue ones plus merge and review events.
pub const PULL_ITEM_TYPES: &str = "[ISSUE_COMMENT, ASSIGNED_EVENT, UNASSIGNED_EVENT, LABELED_EVENT, UNLABELED_EVENT, MILESTONED_EVENT, DEMILESTONED_EVENT, CLOSED_EVENT, REOPENED_EVENT, RENAMED_TITLE_EVENT, MERGED_EVENT, REVIEW_REQUESTED_EVENT, PULL_REQUEST_REVIEW, REFERENCED_EVENT, CROSS_REFERENCED_EVENT]";

/// The node selection of the connection, the same for both.
pub const NODE_SELECTION: &str = "pageInfo { hasNextPage endCursor } nodes { __typename ... on IssueComment { author { login } createdAt body } ... on AssignedEvent { actor { login } createdAt assignee { __typename ... on User { login } } } ... on UnassignedEvent { actor { login } createdAt assignee { __typename ... on User { login } } } ... on LabeledEvent { actor { login } createdAt label { name color } } ... on UnlabeledEvent { actor { login } createdAt label { name color } } ... on MilestonedEvent { actor { login } createdAt milestoneTitle } ... on DemilestonedEvent { actor { login } createdAt milestoneTitle } ... on ClosedEvent { actor { login } createdAt stateReason } ... on ReopenedEvent { actor { login } createdAt } ... on RenamedTitleEvent { actor { login } createdAt previousTitle currentTitle } ... on MergedEvent { actor { login } createdAt } ... on ReviewRequestedEvent { actor { login } createdAt requestedReviewer { __typename ... on User { login } ... on Team { name } } } ... on PullRequestReview { author { login } createdAt state body } ... on ReferencedEvent { actor { login } createdAt commit { abbreviatedOid messageHeadline } } ... on CrossReferencedEvent { actor { login } createdAt source { __typename ... on Issue { number title repository { nameWithOwner } } ... on PullRequest { number title repository { nameWithOwner } } } } }";

/// The `timelineItems` field for a query with an `$after` variable.
pub fn selection(item_types: &str) -> String {
    format!(
        "timelineItems(first: 100, after: $after, itemTypes: {item_types}) {{ {NODE_SELECTION} }}"
    )
}

/// The connection as GitHub sends it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub page_info: PageInfo,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

impl Connection {
    /// Cursor of the next page, `None` on the last page.
    pub fn next_cursor(&self) -> Option<String> {
        if self.page_info.has_next_page {
            self.page_info.end_cursor.clone()
        } else {
            None
        }
    }

    /// The items in order, skipping node types outside the selection and references to
    /// anything but users, teams, issues and pull requests.
    pub fn items(self) -> Vec<TimelineItem> {
        self.nodes.into_iter().filter_map(Node::into_item).collect()
    }
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

#[derive(Deserialize)]
struct LabelNode {
    name: String,
    color: String,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Assignee {
    User {
        login: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Reviewer {
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
#[serde(rename_all = "camelCase")]
struct Commit {
    abbreviated_oid: String,
    message_headline: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    name_with_owner: String,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Source {
    Issue {
        number: u64,
        title: String,
        repository: Repository,
    },
    PullRequest {
        number: u64,
        title: String,
        repository: Repository,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "__typename", rename_all_fields = "camelCase")]
enum Node {
    IssueComment {
        author: Option<Login>,
        created_at: String,
        body: String,
    },
    AssignedEvent {
        actor: Option<Login>,
        created_at: String,
        assignee: Option<Assignee>,
    },
    UnassignedEvent {
        actor: Option<Login>,
        created_at: String,
        assignee: Option<Assignee>,
    },
    LabeledEvent {
        actor: Option<Login>,
        created_at: String,
        label: LabelNode,
    },
    UnlabeledEvent {
        actor: Option<Login>,
        created_at: String,
        label: LabelNode,
    },
    MilestonedEvent {
        actor: Option<Login>,
        created_at: String,
        milestone_title: String,
    },
    DemilestonedEvent {
        actor: Option<Login>,
        created_at: String,
        milestone_title: String,
    },
    ClosedEvent {
        actor: Option<Login>,
        created_at: String,
        state_reason: Option<String>,
    },
    ReopenedEvent {
        actor: Option<Login>,
        created_at: String,
    },
    RenamedTitleEvent {
        actor: Option<Login>,
        created_at: String,
        previous_title: String,
        current_title: String,
    },
    MergedEvent {
        actor: Option<Login>,
        created_at: String,
    },
    ReviewRequestedEvent {
        actor: Option<Login>,
        created_at: String,
        requested_reviewer: Option<Reviewer>,
    },
    PullRequestReview {
        author: Option<Login>,
        created_at: String,
        state: ReviewState,
        body: String,
    },
    ReferencedEvent {
        actor: Option<Login>,
        created_at: String,
        commit: Option<Commit>,
    },
    CrossReferencedEvent {
        actor: Option<Login>,
        created_at: String,
        source: Source,
    },
    #[serde(other)]
    Other,
}

impl Node {
    fn into_item(self) -> Option<TimelineItem> {
        let item = |actor: Option<Login>, created_at: String, event: Event| TimelineItem {
            actor: actor.map(|l| l.login),
            created_at,
            event,
        };
        let label = |l: LabelNode| Label {
            name: l.name,
            color: l.color,
        };
        let user = |a: Option<Assignee>| match a? {
            Assignee::User { login } => Some(login),
            Assignee::Other => None,
        };
        Some(match self {
            Node::IssueComment {
                author,
                created_at,
                body,
            } => item(author, created_at, Event::Comment { body }),
            Node::AssignedEvent {
                actor,
                created_at,
                assignee,
            } => {
                let login = user(assignee)?;
                item(actor, created_at, Event::Assigned { login })
            }
            Node::UnassignedEvent {
                actor,
                created_at,
                assignee,
            } => {
                let login = user(assignee)?;
                item(actor, created_at, Event::Unassigned { login })
            }
            Node::LabeledEvent {
                actor,
                created_at,
                label: l,
            } => item(actor, created_at, Event::Labeled { label: label(l) }),
            Node::UnlabeledEvent {
                actor,
                created_at,
                label: l,
            } => item(actor, created_at, Event::Unlabeled { label: label(l) }),
            Node::MilestonedEvent {
                actor,
                created_at,
                milestone_title,
            } => item(
                actor,
                created_at,
                Event::Milestoned {
                    title: milestone_title,
                },
            ),
            Node::DemilestonedEvent {
                actor,
                created_at,
                milestone_title,
            } => item(
                actor,
                created_at,
                Event::Demilestoned {
                    title: milestone_title,
                },
            ),
            Node::ClosedEvent {
                actor,
                created_at,
                state_reason,
            } => item(
                actor,
                created_at,
                Event::Closed {
                    reason: state_reason,
                },
            ),
            Node::ReopenedEvent { actor, created_at } => item(actor, created_at, Event::Reopened),
            Node::RenamedTitleEvent {
                actor,
                created_at,
                previous_title,
                current_title,
            } => item(
                actor,
                created_at,
                Event::Renamed {
                    from: previous_title,
                    to: current_title,
                },
            ),
            Node::MergedEvent { actor, created_at } => item(actor, created_at, Event::Merged),
            Node::ReviewRequestedEvent {
                actor,
                created_at,
                requested_reviewer,
            } => {
                let reviewer = match requested_reviewer? {
                    Reviewer::User { login } => login,
                    Reviewer::Team { name } => name,
                    Reviewer::Other => return None,
                };
                item(actor, created_at, Event::ReviewRequested { reviewer })
            }
            Node::PullRequestReview {
                author,
                created_at,
                state,
                body,
            } => item(author, created_at, Event::Reviewed { state, body }),
            Node::ReferencedEvent {
                actor,
                created_at,
                commit,
            } => {
                let commit = commit?;
                item(
                    actor,
                    created_at,
                    Event::Referenced {
                        id: commit.abbreviated_oid,
                        headline: commit.message_headline,
                    },
                )
            }
            Node::CrossReferencedEvent {
                actor,
                created_at,
                source,
            } => {
                let (number, title, repository) = match source {
                    Source::Issue {
                        number,
                        title,
                        repository,
                    }
                    | Source::PullRequest {
                        number,
                        title,
                        repository,
                    } => (number, title, repository.name_with_owner),
                    Source::Other => return None,
                };
                item(
                    actor,
                    created_at,
                    Event::CrossReferenced {
                        number,
                        title,
                        repository,
                    },
                )
            }
            Node::Other => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NODES: &str = r#"{"pageInfo":{"hasNextPage":true,"endCursor":"cur"},"nodes":[
        {"__typename":"IssueComment","author":{"login":"a"},"createdAt":"2026-09-01T00:00:00Z","body":"LGTM"},
        {"__typename":"AssignedEvent","actor":{"login":"octo"},"createdAt":"2026-09-02T00:00:00Z","assignee":{"__typename":"User","login":"b"}},
        {"__typename":"UnassignedEvent","actor":null,"createdAt":"2026-09-02T00:00:01Z","assignee":{"__typename":"User","login":"b"}},
        {"__typename":"AssignedEvent","actor":{"login":"octo"},"createdAt":"2026-09-02T00:00:02Z","assignee":{"__typename":"Bot"}},
        {"__typename":"LabeledEvent","actor":{"login":"octo"},"createdAt":"2026-09-03T00:00:00Z","label":{"name":"bug","color":"d73a4a"}},
        {"__typename":"UnlabeledEvent","actor":{"login":"octo"},"createdAt":"2026-09-03T00:00:01Z","label":{"name":"bug","color":"d73a4a"}},
        {"__typename":"MilestonedEvent","actor":{"login":"octo"},"createdAt":"2026-09-04T00:00:00Z","milestoneTitle":"v1"},
        {"__typename":"DemilestonedEvent","actor":{"login":"octo"},"createdAt":"2026-09-04T00:00:01Z","milestoneTitle":"v1"},
        {"__typename":"ClosedEvent","actor":{"login":"octo"},"createdAt":"2026-09-05T00:00:00Z","stateReason":"COMPLETED"},
        {"__typename":"ClosedEvent","actor":{"login":"octo"},"createdAt":"2026-09-05T00:00:00Z","stateReason":null},
        {"__typename":"ReopenedEvent","actor":{"login":"octo"},"createdAt":"2026-09-06T00:00:00Z"},
        {"__typename":"RenamedTitleEvent","actor":{"login":"octo"},"createdAt":"2026-09-07T00:00:00Z","previousTitle":"Old","currentTitle":"New"},
        {"__typename":"MergedEvent","actor":{"login":"octo"},"createdAt":"2026-09-08T00:00:00Z"},
        {"__typename":"ReviewRequestedEvent","actor":{"login":"octo"},"createdAt":"2026-09-09T00:00:00Z","requestedReviewer":{"__typename":"Team","name":"core"}},
        {"__typename":"ReviewRequestedEvent","actor":{"login":"octo"},"createdAt":"2026-09-09T00:00:01Z","requestedReviewer":null},
        {"__typename":"PullRequestReview","author":{"login":"rev"},"createdAt":"2026-09-10T00:00:00Z","state":"APPROVED","body":"ship it"},
        {"__typename":"ReferencedEvent","actor":{"login":"octo"},"createdAt":"2026-09-11T00:00:00Z","commit":{"abbreviatedOid":"abc1234","messageHeadline":"Fix it"}},
        {"__typename":"ReferencedEvent","actor":{"login":"octo"},"createdAt":"2026-09-11T00:00:01Z","commit":null},
        {"__typename":"CrossReferencedEvent","actor":{"login":"octo"},"createdAt":"2026-09-12T00:00:00Z","source":{"__typename":"PullRequest","number":9,"title":"Follow-up","repository":{"nameWithOwner":"o/r"}}},
        {"__typename":"SubscribedEvent","createdAt":"2026-09-13T00:00:00Z"}
    ]}"#;

    #[test]
    /// GH-R-017, GH-E-008 — every event type is carried with actor and time; unknown types, non-user assignees, missing reviewers and commits are skipped; the cursor follows `hasNextPage`.
    fn ut_items_of_every_type() {
        let connection: Connection = serde_json::from_str(NODES).expect("decodes");
        assert_eq!(connection.next_cursor().as_deref(), Some("cur"));
        let items = connection.items();
        let events: Vec<&Event> = items.iter().map(|i| &i.event).collect();
        let bug = Label {
            name: "bug".into(),
            color: "d73a4a".into(),
        };
        assert_eq!(
            events,
            vec![
                &Event::Comment {
                    body: "LGTM".into()
                },
                &Event::Assigned { login: "b".into() },
                &Event::Unassigned { login: "b".into() },
                &Event::Labeled { label: bug.clone() },
                &Event::Unlabeled { label: bug },
                &Event::Milestoned { title: "v1".into() },
                &Event::Demilestoned { title: "v1".into() },
                &Event::Closed {
                    reason: Some("COMPLETED".into())
                },
                &Event::Closed { reason: None },
                &Event::Reopened,
                &Event::Renamed {
                    from: "Old".into(),
                    to: "New".into()
                },
                &Event::Merged,
                &Event::ReviewRequested {
                    reviewer: "core".into()
                },
                &Event::Reviewed {
                    state: ReviewState::Approved,
                    body: "ship it".into()
                },
                &Event::Referenced {
                    id: "abc1234".into(),
                    headline: "Fix it".into()
                },
                &Event::CrossReferenced {
                    number: 9,
                    title: "Follow-up".into(),
                    repository: "o/r".into()
                },
            ]
        );
        assert_eq!(
            items[0].actor.as_deref(),
            Some("a"),
            "a comment's author is its actor"
        );
        assert_eq!(items[0].created_at, "2026-09-01T00:00:00Z");
        assert_eq!(items[2].actor, None);
        assert_eq!(
            items[13].actor.as_deref(),
            Some("rev"),
            "a review's author is its actor"
        );
        let last: Connection =
            serde_json::from_str(&NODES.replace(r#""hasNextPage":true"#, r#""hasNextPage":false"#))
                .expect("decodes");
        assert_eq!(last.next_cursor(), None);
    }

    #[test]
    /// GH-R-010, GH-R-015 — the selection carries the type filter and every node fragment.
    fn ut_selection_text() {
        let pull = selection(PULL_ITEM_TYPES);
        assert!(
            pull.starts_with(
                "timelineItems(first: 100, after: $after, itemTypes: [ISSUE_COMMENT, "
            ),
            "{pull}"
        );
        assert!(
            pull.contains("MERGED_EVENT") && pull.contains("PULL_REQUEST_REVIEW"),
            "{pull}"
        );
        let issue = selection(ISSUE_ITEM_TYPES);
        assert!(
            !issue.contains("MERGED_EVENT") && !issue.contains("REVIEW"),
            "{issue}"
        );
        for fragment in [
            "... on IssueComment { author { login } createdAt body }",
            "... on ClosedEvent { actor { login } createdAt stateReason }",
            "... on RenamedTitleEvent { actor { login } createdAt previousTitle currentTitle }",
            "... on ReferencedEvent { actor { login } createdAt commit { abbreviatedOid messageHeadline } }",
            "... on CrossReferencedEvent { actor { login } createdAt source { __typename ... on Issue { number title repository { nameWithOwner } } ... on PullRequest { number title repository { nameWithOwner } } } }",
        ] {
            assert!(issue.contains(fragment), "{fragment}");
        }
    }
}
