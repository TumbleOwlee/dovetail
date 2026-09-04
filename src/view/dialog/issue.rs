//! Issue details as overlay content.

use crate::github::issue::{Issue, IssueState};
use crate::view::dialog::details::{DetailsContent, SidebarBox};
use crate::view::dialog::pull::{badges, logins, plain};

pub const LOADING: &str = "Loading issue..";

/// The overlay content of an issue: seven boxes, assignees first.
pub fn content(issue: Issue) -> DetailsContent {
    let state = match issue.state {
        IssueState::Open => "open",
        IssueState::Closed => "closed",
    };
    DetailsContent {
        title: issue.title,
        state,
        author: issue.author,
        body: issue.body,
        timeline: issue.timeline,
        boxes: vec![
            SidebarBox {
                title: "Assignees",
                lines: logins(&issue.assignees),
            },
            SidebarBox {
                title: "Labels",
                lines: badges(&issue.labels),
            },
            SidebarBox {
                title: "Projects",
                lines: plain(&issue.projects),
            },
            SidebarBox {
                title: "Milestones",
                lines: plain(issue.milestone.as_slice()),
            },
            SidebarBox {
                title: "Relationships",
                lines: plain(&issue.relationships),
            },
            SidebarBox {
                title: "Development",
                lines: plain(&issue.development),
            },
            SidebarBox {
                title: "Participants",
                lines: logins(&issue.participants),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::board::Label;
    use crate::github::timeline::{Event, TimelineItem};

    #[test]
    /// TU-R-060 — state and author carried; seven boxes in order with logins, badges, references and relationships.
    fn ut_issue_content() {
        let issue = Issue {
            number: 7,
            title: "Crash on start".into(),
            state: IssueState::Closed,
            body: "Steps".into(),
            url: "u".into(),
            author: None,
            repository: "o/r".into(),
            labels: vec![Label {
                name: "bug".into(),
                color: "d73a4a".into(),
            }],
            assignees: vec!["a".into()],
            projects: vec!["Roadmap".into()],
            milestone: Some("v1".into()),
            relationships: vec!["parent #3 Epic".into(), "sub #8 Child".into()],
            development: vec!["#5 Fix crash".into()],
            participants: vec!["octo".into()],
            timeline: vec![TimelineItem {
                actor: Some("a".into()),
                created_at: "2026-09-04T10:00:00Z".into(),
                event: Event::Comment {
                    body: "LGTM".into(),
                },
            }],
        };
        let c = content(issue);
        assert_eq!(
            (c.state, c.author, c.title.as_str()),
            ("closed", None, "Crash on start")
        );
        assert_eq!(c.timeline.len(), 1);
        let titles: Vec<&str> = c.boxes.iter().map(|b| b.title).collect();
        assert_eq!(
            titles,
            vec![
                "Assignees",
                "Labels",
                "Projects",
                "Milestones",
                "Relationships",
                "Development",
                "Participants"
            ]
        );
        let text =
            |b: &SidebarBox| -> Vec<String> { b.lines.iter().map(|l| l.to_string()).collect() };
        assert_eq!(text(&c.boxes[0]), vec!["@a"]);
        assert_eq!(text(&c.boxes[1]), vec![" bug "]);
        assert_eq!(text(&c.boxes[3]), vec!["v1"]);
        assert_eq!(text(&c.boxes[4]), vec!["parent #3 Epic", "sub #8 Child"]);
        assert_eq!(text(&c.boxes[5]), vec!["#5 Fix crash"]);
        assert_eq!(text(&c.boxes[6]), vec!["@octo"]);
    }
}
