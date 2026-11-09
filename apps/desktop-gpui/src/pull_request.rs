//! The web's pull request reads (`packages/contracts/src/pullRequest.ts`): the payloads the
//! panel sends and the way a failure is told apart.

#![expect(dead_code, reason = "the pull request surface consumes these next")]

use crate::rpc::split_not_found;
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
pub fn detail_failure(message: &str, number: u64) -> Unavailable {
    match split_not_found(message) {
        (_, true) => Unavailable {
            title: format!("Pull request #{number} not found"),
            error: "It may be an issue rather than a pull request, or this account can't see it."
                .to_owned(),
        },
        (text, false) => Unavailable {
            title: UNAVAILABLE_TITLE.to_owned(),
            error: text.to_owned(),
        },
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
        let missing = detail_failure(&format!("{NOT_FOUND_MARK}gone"), 7);
        assert_eq!(missing.title, "Pull request #7 not found");
        let other = detail_failure("paused", 7);
        assert_eq!(other.title, UNAVAILABLE_TITLE);
        assert_eq!(other.error, "paused");
    }
}
