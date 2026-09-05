//! Pull request details as overlay content.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::github::pull::{PullDetails, ReviewState};
use crate::github::pulls::PullState;
use crate::view::board::{badge_text_color, label_color};
use crate::view::dialog::details::{DetailsContent, Link, Panes, SidebarBox};
use crate::view::theme;

pub const LOADING: &str = "Loading pull request..";

fn review_state(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Pending => "pending",
        ReviewState::Approved => "approved",
        ReviewState::ChangesRequested => "changes requested",
        ReviewState::Commented => "commented",
        ReviewState::Dismissed => "dismissed",
    }
}

/// `@<login>` lines.
pub fn logins(names: &[String]) -> Vec<Line<'static>> {
    names.iter().map(|n| Line::raw(format!("@{n}"))).collect()
}

/// Plain text lines.
pub fn plain(items: &[String]) -> Vec<Line<'static>> {
    items.iter().map(|i| Line::raw(i.clone())).collect()
}

/// One colored badge per label.
pub fn badges(labels: &[crate::github::board::Label]) -> Vec<Line<'static>> {
    labels
        .iter()
        .map(|l| {
            let bg = label_color(&l.color).unwrap_or(theme::TEMPLATE.hi_bg);
            Line::from(Span::styled(
                format!(" {} ", l.name),
                Style::default().fg(badge_text_color(bg)).bg(bg),
            ))
        })
        .collect()
}

/// The overlay content of a pull request: seven boxes, reviewers first.
pub fn content(details: PullDetails) -> DetailsContent {
    let (owner, repo) = details
        .repository
        .split_once('/')
        .unwrap_or((details.repository.as_str(), ""));
    let reviewers = details
        .reviewers
        .iter()
        .map(|r| {
            Line::from(vec![
                Span::raw(format!("@{} ", r.name)),
                Span::styled(
                    review_state(r.state),
                    theme::on_bg(theme::TEMPLATE.placeholder),
                ),
            ])
        })
        .collect();
    let state = match (details.state, details.draft) {
        (PullState::Open, false) => "open",
        (PullState::Open, true) => "draft",
        (PullState::Merged, _) => "merged",
        (PullState::Closed, _) => "closed",
    };
    DetailsContent {
        panes: Panes::Pull {
            commits: details.commits,
            files: details.files,
            owner: owner.to_string(),
            repo: repo.to_string(),
            head_oid: details.head_oid,
        },
        title: details.title,
        state,
        author: details.author,
        body: details.body,
        timeline: details.timeline,
        boxes: vec![
            SidebarBox {
                title: "Reviewers",
                lines: reviewers,
                links: vec![],
            },
            SidebarBox {
                title: "Assignees",
                lines: logins(&details.assignees),
                links: vec![],
            },
            SidebarBox {
                title: "Labels",
                lines: badges(&details.labels),
                links: vec![],
            },
            SidebarBox {
                title: "Projects",
                lines: plain(&details.projects),
                links: vec![],
            },
            SidebarBox {
                title: "Milestone",
                lines: plain(details.milestone.as_slice()),
                links: vec![],
            },
            SidebarBox {
                title: "Development",
                lines: details
                    .development
                    .iter()
                    .map(|i| Line::raw(format!("#{} {}", i.number, i.title)))
                    .collect(),
                links: details
                    .development
                    .into_iter()
                    .map(|i| Link::Issue {
                        id: i.id,
                        number: i.number,
                        title: i.title,
                    })
                    .collect(),
            },
            SidebarBox {
                title: "Participants",
                lines: logins(&details.participants),
                links: vec![],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::board::Label;
    use crate::github::pull::Reviewer;
    use crate::github::pull::{CommitAuthor, IssueRef};

    #[test]
    /// TU-R-066, TU-R-068 — state and author carried; seven boxes in order with reviewer states, `@` logins, label badges and references.
    fn ut_pull_content() {
        let commit = crate::github::pull::Commit {
            sha: "abc".into(),
            headline: "h".into(),
            author: CommitAuthor::User("o".into()),
            date: "d".into(),
        };
        let file = crate::github::files::ChangedFile {
            path: "a".into(),
            previous_path: None,
            status: crate::github::files::FileStatus::Added,
            additions: 1,
            deletions: 0,
            patch: None,
        };
        let details = PullDetails {
            number: 5,
            title: "Fix crash".into(),
            body: "b".into(),
            state: PullState::Open,
            draft: true,
            head_oid: "abc".into(),
            repository: "o/r".into(),
            author: Some("octo".into()),
            reviewers: vec![
                Reviewer {
                    name: "rev".into(),
                    state: ReviewState::Pending,
                },
                Reviewer {
                    name: "a".into(),
                    state: ReviewState::ChangesRequested,
                },
            ],
            assignees: vec!["b".into()],
            labels: vec![Label {
                name: "bug".into(),
                color: "d73a4a".into(),
            }],
            projects: vec!["Roadmap".into()],
            milestone: Some("v1".into()),
            development: vec![IssueRef {
                id: "I_7".into(),
                number: 7,
                title: "Crash on start".into(),
            }],
            commits: vec![commit.clone()],
            files: vec![file.clone()],
            participants: vec!["octo".into(), "a".into()],
            ..PullDetails::default()
        };
        let c = content(details);
        assert_eq!(
            (c.state, c.author.as_deref(), c.title.as_str()),
            ("draft", Some("octo"), "Fix crash")
        );
        let titles: Vec<&str> = c.boxes.iter().map(|b| b.title).collect();
        assert_eq!(
            titles,
            vec![
                "Reviewers",
                "Assignees",
                "Labels",
                "Projects",
                "Milestone",
                "Development",
                "Participants"
            ]
        );
        let text =
            |b: &SidebarBox| -> Vec<String> { b.lines.iter().map(|l| l.to_string()).collect() };
        assert_eq!(
            text(&c.boxes[0]),
            vec!["@rev pending", "@a changes requested"]
        );
        assert_eq!(text(&c.boxes[5]), vec!["#7 Crash on start"]);
        assert_eq!(
            c.panes,
            Panes::Pull {
                commits: vec![commit.clone()],
                files: vec![file.clone()],
                owner: "o".into(),
                repo: "r".into(),
                head_oid: "abc".into(),
            }
        );
        assert_eq!(
            c.boxes[5].links,
            vec![Link::Issue {
                id: "I_7".into(),
                number: 7,
                title: "Crash on start".into()
            }]
        );
        assert!(
            c.boxes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 5 || b.links.is_empty())
        );
        assert_eq!(text(&c.boxes[1]), vec!["@b"]);
        assert_eq!(text(&c.boxes[2]), vec![" bug "]);
        assert_eq!(text(&c.boxes[3]), vec!["Roadmap"]);
        assert_eq!(text(&c.boxes[4]), vec!["v1"]);
        assert_eq!(text(&c.boxes[5]), vec!["#7 Crash on start"]);
        assert_eq!(text(&c.boxes[6]), vec!["@octo", "@a"]);
        let merged = content(PullDetails {
            state: PullState::Merged,
            ..PullDetails::default()
        });
        assert_eq!(merged.state, "merged");
        assert!(merged.boxes[4].lines.is_empty(), "no milestone");
    }
}
