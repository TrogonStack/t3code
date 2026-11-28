//! The web's pull request reads (`packages/contracts/src/pullRequest.ts`): the payloads the
//! panel sends and the way a failure is told apart.

use crate::rpc::split_not_found;
use crate::status::PrState;
use serde::Deserialize;
use serde_json::{Value, json};

/// `PullRequestRef`: the project, the repository on its host and the number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Reference {
    pub project_id: String,
    pub host: Option<String>,
    pub repository: String,
    pub number: u64,
}

impl Reference {
    /// `pullRequestPanelReference` (the web keys queries on this field order).
    pub fn payload(&self) -> Value {
        let mut payload = json!({
            "projectId": self.project_id,
            "repository": self.repository,
            "number": self.number,
        });
        if let Some(host) = self.host.as_ref().filter(|host| !host.is_empty()) {
            payload["host"] = json!(host);
        }
        payload
    }

    /// `usePullRequestDetail` reads the detail with `allowStale: false`.
    pub fn fresh_payload(&self) -> Value {
        let mut payload = self.payload();
        payload["allowStale"] = json!(false);
        payload
    }

    /// `pullRequestRefKey`.
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}#{}",
            self.project_id,
            self.host.as_deref().unwrap_or(""),
            self.repository,
            self.number
        )
    }
}

/// `PullRequestListInput` for the page's first read of one state.
pub fn list_payload(state: &str, project_ids: &[String], limit: u32) -> Value {
    let mut payload = json!({ "state": state, "limit": limit });
    if !project_ids.is_empty() {
        payload["projectIds"] = json!(project_ids);
    }
    payload
}

/// What a failed read shows: `PullRequestsUnavailableState`'s title and description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable {
    pub title: String,
    pub error: String,
}

pub const UNAVAILABLE_TITLE: &str = "Could not load pull requests";

/// The detail panel's failure: a not-found reason gets its own title and copy.
pub fn detail_failure(message: &str, number: u64, host: &str) -> Unavailable {
    let term = term(host);
    match split_not_found(message) {
        (_, true) => Unavailable {
            title: format!("{} #{number} not found", capitalize(term)),
            error: format!(
                "It may be an issue rather than a {term}, or this account can't see it."
            ),
        },
        (text, false) => Unavailable {
            title: UNAVAILABLE_TITLE.to_owned(),
            error: text.to_owned(),
        },
    }
}

/// The web's provider terminology for a host or URL: GitLab says "merge request". The web
/// resolves the provider from the repository identity; the host name stands in for it.
pub fn term(host_or_url: &str) -> &'static str {
    let host = host_or_url
        .split_once("://")
        .map_or(host_or_url, |(_, rest)| rest)
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if host.contains("gitlab") {
        "merge request"
    } else {
        "pull request"
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// `PullRequestDetail`, the fields the panel shows. The server's other fields are left out.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub url: String,
    #[serde(default)]
    pub author: Option<Actor>,
    pub state: State,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub mergeability: Option<String>,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub changed_files: u64,
    pub head_branch: String,
    pub base_branch: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub reviewers: Vec<Actor>,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub checks: Vec<Check>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Open,
    Closed,
    Merged,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Actor {
    pub login: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Label {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CheckStatus {
    Pending,
    ActionRequired,
    Success,
    Failure,
    Skipped,
    Neutral,
    Cancelled,
}

impl CheckStatus {
    /// `CHECK_STATUS_PRESENTATION`'s labels.
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Running",
            Self::ActionRequired => "Awaiting action",
            Self::Success => "Passed",
            Self::Failure => "Failed",
            Self::Cancelled => "Cancelled",
            Self::Skipped => "Skipped",
            Self::Neutral => "Neutral",
        }
    }
}

impl Detail {
    pub fn decode(value: &Value) -> Option<Self> {
        Self::deserialize(value).ok()
    }

    pub fn badge_state(&self) -> PrState {
        badge_state(self.state, self.is_draft)
    }

    pub fn state_label(&self) -> &'static str {
        match self.badge_state() {
            PrState::Draft => "Draft",
            PrState::Closed => "Closed",
            PrState::Merged => "Merged",
            PrState::Open | PrState::Pending => "Open",
        }
    }

    /// `resolvePullRequestConflict`.
    pub fn conflict(&self) -> Option<String> {
        (self.state == State::Open
            && !self.is_draft
            && self.mergeability.as_deref() == Some("conflicting"))
        .then(|| {
            if self.base_branch.is_empty() {
                "Has conflicts".to_owned()
            } else {
                format!("Conflicts with {}", self.base_branch)
            }
        })
    }
}

/// `resolvePullRequestState`: an open draft is its own state.
pub fn badge_state(state: State, is_draft: bool) -> PrState {
    match (state, is_draft) {
        (State::Open, true) => PrState::Draft,
        (State::Open, false) => PrState::Open,
        (State::Closed, _) => PrState::Closed,
        (State::Merged, _) => PrState::Merged,
    }
}

/// `PullRequestListEntry`, the fields a row shows.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListEntry {
    pub project_id: String,
    pub host: String,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub author: Option<Actor>,
    pub head_branch: String,
    pub base_branch: String,
    pub state: State,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
}

impl ListEntry {
    pub fn reference(&self) -> Reference {
        Reference {
            project_id: self.project_id.clone(),
            host: Some(self.host.clone()).filter(|host| !host.is_empty()),
            repository: self.repository.clone(),
            number: self.number,
        }
    }
}

/// `PullRequestListProjectError`: a project whose host could not be read.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListError {
    pub project_title: String,
    pub message: String,
}

/// `PullRequestListResult`, newest update first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    pub entries: Vec<ListEntry>,
    pub errors: Vec<ListError>,
    pub truncated: bool,
}

impl List {
    /// Entries that do not decode are dropped, so one odd row cannot hide the rest.
    pub fn decode(value: &Value) -> Option<Self> {
        let items = |key: &str| value.get(key).and_then(Value::as_array);
        let mut entries: Vec<ListEntry> = items("entries")?
            .iter()
            .filter_map(|entry| ListEntry::deserialize(entry).ok())
            .collect();
        entries.sort_by_key(|entry| {
            std::cmp::Reverse(crate::shell::timestamp_ms(&entry.updated_at).unwrap_or(0))
        });
        let errors = items("errors")
            .into_iter()
            .flatten()
            .filter_map(|error| ListError::deserialize(error).ok())
            .collect();
        Some(Self {
            entries,
            errors,
            truncated: value.get("truncated").and_then(Value::as_bool) == Some(true),
        })
    }
}

/// How long ago `then` was, in the largest whole unit, for the "updated" meta.
pub fn relative_age(then_ms: i64, now_ms: i64) -> String {
    let seconds = ((now_ms - then_ms) / 1000).max(0);
    let (count, unit) = match seconds {
        0..=59 => return "just now".to_owned(),
        60..=3599 => (seconds / 60, "minute"),
        3600..=86399 => (seconds / 3600, "hour"),
        86400..=2_591_999 => (seconds / 86400, "day"),
        2_592_000..=31_535_999 => (seconds / 2_592_000, "month"),
        _ => (seconds / 31_536_000, "year"),
    };
    format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" })
}

/// `openOnHostLabel` for the host a pull request URL names. The web resolves the provider
/// from the repository identity; the URL's host stands in for it, so a self-hosted instance
/// reads "Open on host".
pub fn open_label(url: &str) -> String {
    let host = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    match host {
        "github.com" => "Open on GitHub".to_owned(),
        "gitlab.com" => "Open on GitLab".to_owned(),
        "bitbucket.org" => "Open on Bitbucket".to_owned(),
        "dev.azure.com" => "Open on Azure DevOps".to_owned(),
        _ => "Open on host".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::NOT_FOUND_MARK;

    fn reference(host: Option<&str>) -> Reference {
        Reference {
            project_id: "p1".to_owned(),
            host: host.map(str::to_owned),
            repository: "owner/repo".to_owned(),
            number: 7,
        }
    }

    #[test]
    fn the_reference_carries_a_host_only_when_known() {
        assert_eq!(
            reference(None).payload(),
            json!({"projectId": "p1", "repository": "owner/repo", "number": 7})
        );
        assert_eq!(
            reference(Some("example.test")).payload()["host"],
            "example.test"
        );
        assert_eq!(reference(Some("")).payload().get("host"), None);
        assert_eq!(reference(None).fresh_payload()["allowStale"], false);
        assert_eq!(reference(Some("h")).key(), "p1:h:owner/repo#7");
    }

    #[test]
    fn a_missing_pull_request_has_its_own_title() {
        let missing = detail_failure(&format!("{NOT_FOUND_MARK}gone"), 7, "github.com");
        assert_eq!(missing.title, "Pull request #7 not found");
        let gitlab = detail_failure(&format!("{NOT_FOUND_MARK}gone"), 7, "gitlab.com");
        assert_eq!(gitlab.title, "Merge request #7 not found");
        let other = detail_failure("paused", 7, "");
        assert_eq!(other.title, UNAVAILABLE_TITLE);
        assert_eq!(other.error, "paused");
    }

    #[test]
    fn the_open_label_names_the_known_hosts() {
        assert_eq!(
            open_label("https://github.com/o/r/pull/1"),
            "Open on GitHub"
        );
        assert_eq!(open_label("https://www.gitlab.com/o/r"), "Open on GitLab");
        assert_eq!(open_label("https://git.example.test/o/r"), "Open on host");
        assert_eq!(open_label(""), "Open on host");
    }

    #[test]
    fn the_detail_decodes_what_the_panel_shows() {
        let detail = Detail::decode(&json!({
            "title": "Fix it", "url": "https://github.com/o/r/pull/7", "state": "open",
            "isDraft": true, "headBranch": "topic", "baseBranch": "main",
            "mergeability": "conflicting", "additions": 3, "deletions": 1, "changedFiles": 2,
            "author": {"login": "someone", "name": null, "avatarUrl": null},
            "labels": [{"name": "bug", "color": null}],
            "checks": [{"name": "ci", "status": "action-required", "description": null, "url": null}],
            "provider": "github", "unknownField": 1,
        }))
        .expect("decodes");
        assert_eq!(detail.badge_state(), PrState::Draft);
        assert_eq!(detail.conflict(), None);
        assert_eq!(detail.checks[0].status, CheckStatus::ActionRequired);
        assert_eq!(detail.checks[0].status.label(), "Awaiting action");
        assert_eq!(detail.author.expect("author").login, "someone");
        assert!(Detail::decode(&json!({"title": "x"})).is_none());
    }

    #[test]
    fn only_an_open_ready_pull_request_reports_conflicts() {
        let mut detail = Detail::decode(&json!({
            "title": "t", "url": "u", "state": "open", "headBranch": "h",
            "baseBranch": "main", "mergeability": "conflicting",
        }))
        .expect("decodes");
        assert_eq!(detail.conflict().as_deref(), Some("Conflicts with main"));
        detail.state = State::Merged;
        assert_eq!(detail.conflict(), None);
    }

    #[test]
    fn ages_use_the_largest_whole_unit() {
        assert_eq!(relative_age(0, 30_000), "just now");
        assert_eq!(relative_age(0, 60_000), "1 minute ago");
        assert_eq!(relative_age(0, 3 * 3_600_000), "3 hours ago");
        assert_eq!(relative_age(0, 2 * 86_400_000), "2 days ago");
        assert_eq!(relative_age(10, 0), "just now");
    }

    #[test]
    fn the_list_decodes_rows_newest_first_and_drops_odd_ones() {
        let row = |number: u64, updated: &str| {
            json!({
                "projectId": "p", "host": "github.com", "repository": "o/r", "number": number,
                "title": "t", "url": "u", "headBranch": "h", "baseBranch": "b",
                "state": "merged", "updatedAt": updated, "provider": "github",
            })
        };
        let list = List::decode(&json!({
            "entries": [
                row(1, "2026-01-01T00:00:00.000Z"),
                {"number": 9},
                row(2, "2026-02-01T00:00:00.000Z"),
            ],
            "errors": [{"projectId": "p", "projectTitle": "Proj", "message": "no cli"}],
            "truncated": true,
        }))
        .expect("decodes");
        let numbers: Vec<u64> = list.entries.iter().map(|entry| entry.number).collect();
        assert_eq!(numbers, [2, 1]);
        assert_eq!(list.errors[0].project_title, "Proj");
        assert!(list.truncated);
        assert_eq!(list.entries[0].reference().key(), "p:github.com:o/r#2");
        assert!(List::decode(&json!({})).is_none());
    }
}
