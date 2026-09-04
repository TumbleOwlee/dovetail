//! A pull request's changed files with their patches, from the REST API.

use serde::Deserialize;

use crate::github::GithubError;

const PER_PAGE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Added,
    Removed,
    Modified,
    Renamed,
    Copied,
    Changed,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    /// The path before a rename or copy.
    pub previous_path: Option<String>,
    pub status: FileStatus,
    pub additions: u64,
    pub deletions: u64,
    /// The unified diff; `None` for binaries and diffs GitHub does not send.
    pub patch: Option<String>,
}

/// The URL of one page of the files listing.
pub fn page_url(owner: &str, repo: &str, number: u64, page: usize) -> String {
    format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls/{number}/files?per_page={PER_PAGE}&page={page}"
    )
}

/// The files of one page.
pub fn parse_files(body: &str) -> Result<Vec<ChangedFile>, GithubError> {
    let entries: Vec<Entry> =
        serde_json::from_str(body).map_err(|e| GithubError::Decode(e.to_string()))?;
    Ok(entries
        .into_iter()
        .map(|e| ChangedFile {
            path: e.filename,
            previous_path: e.previous_filename,
            status: e.status.unwrap_or(FileStatus::Changed),
            additions: e.additions,
            deletions: e.deletions,
            patch: e.patch,
        })
        .collect())
}

/// One entry of the listing; an unknown `status` decodes as `None`.
#[derive(Deserialize)]
struct Entry {
    filename: String,
    previous_filename: Option<String>,
    #[serde(deserialize_with = "lenient_status")]
    status: Option<FileStatus>,
    additions: u64,
    deletions: u64,
    patch: Option<String>,
}

fn lenient_status<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<FileStatus>, D::Error> {
    let raw = String::deserialize(d)?;
    Ok(serde_json::from_value(serde_json::Value::String(raw)).ok())
}

/// Every changed file, paging while a page is full.
pub async fn load_changed_files(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<Vec<ChangedFile>, GithubError> {
    let mut files = Vec::new();
    for page in 1.. {
        let response = client
            .get(page_url(owner, repo, number, page))
            .bearer_auth(token)
            .header(reqwest::header::USER_AGENT, "prodgy")
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(GithubError::Status(status.as_u16()));
        }
        let batch = parse_files(&response.text().await?)?;
        let full = batch.len() >= PER_PAGE;
        files.extend(batch);
        if !full {
            break;
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"[{"sha":"1","filename":"src/main.rs","status":"modified","additions":2,"deletions":1,"changes":3,"blob_url":"b","raw_url":"r","contents_url":"c","patch":"@@ -1,2 +1,3 @@\n fn main() {\n-    old();\n+    new();\n+    more();\n }"},
        {"sha":"2","filename":"docs/new.md","status":"renamed","additions":0,"deletions":0,"changes":0,"blob_url":"b","raw_url":"r","contents_url":"c","previous_filename":"docs/old.md"},
        {"sha":"3","filename":"logo.png","status":"weird","additions":0,"deletions":0,"changes":0,"blob_url":"b","raw_url":"r","contents_url":"c"}]"#;

    #[test]
    /// GH-R-019 — the page URL names the pull request, 100 per page and the page number.
    fn ut_page_url() {
        assert_eq!(
            page_url("o", "r", 5, 2),
            "https://api.github.com/repos/o/r/pulls/5/files?per_page=100&page=2"
        );
    }

    #[test]
    /// GH-R-019, GH-E-011 — files decode with their patch and previous path when present; an unknown status is `changed`.
    fn ut_parse_files() {
        let files = parse_files(BODY).expect("decodes");
        assert_eq!(
            files,
            vec![
                ChangedFile {
                    path: "src/main.rs".into(),
                    previous_path: None,
                    status: FileStatus::Modified,
                    additions: 2,
                    deletions: 1,
                    patch: Some(
                        "@@ -1,2 +1,3 @@\n fn main() {\n-    old();\n+    new();\n+    more();\n }"
                            .into()
                    ),
                },
                ChangedFile {
                    path: "docs/new.md".into(),
                    previous_path: Some("docs/old.md".into()),
                    status: FileStatus::Renamed,
                    additions: 0,
                    deletions: 0,
                    patch: None,
                },
                ChangedFile {
                    path: "logo.png".into(),
                    previous_path: None,
                    status: FileStatus::Changed,
                    additions: 0,
                    deletions: 0,
                    patch: None,
                },
            ]
        );
        assert!(parse_files("[]").expect("empty page").is_empty());
    }

    #[test]
    /// GH-R-019 — garbage, a truncated body and an object instead of a list are decode errors.
    fn ut_parse_files_errors() {
        for body in [
            "",
            "not json",
            &BODY[..BODY.len() / 2],
            r#"{"message":"Not Found"}"#,
        ] {
            assert!(
                matches!(parse_files(body), Err(GithubError::Decode(_))),
                "{body}"
            );
        }
    }

    #[tokio::test]
    /// GH-R-019 — an unreachable host is a typed HTTP error.
    async fn ut_load_unreachable_is_http_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).expect("proxy url"))
            .build()
            .expect("client");
        let result = load_changed_files(&client, "t", "o", "r", 1).await;
        assert!(matches!(result, Err(GithubError::Http(_))), "{result:?}");
    }
}
