//! The Scheduled tasks tab's list (`ScheduledTasksSettings.tsx`, `scheduledTasksSettings.logic.ts`):
//! decoding the task stream, the schedule and status lines of a row and which environment and
//! project selection a task belongs to.

use crate::settings_labs::{Environment, Scope, ScopeData};
use crate::usage::parse_timestamp;
use crate::usage_data::{Zone, format_date_time_medium};
use serde_json::Value;
use std::collections::HashMap;

/// Call tokens, above every other range.
pub const TOKEN_BASE: u64 = 1 << 54;

const WEEKDAY_LABELS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Environment id to the scope's project keys to the project ids behind each key.
pub type ProjectsByEnvironment = HashMap<String, HashMap<String, Vec<String>>>;

#[derive(Clone, Debug, PartialEq)]
pub enum Schedule {
    Interval {
        every_ms: f64,
    },
    FixedTime {
        time_of_day: String,
        weekdays: Vec<i64>,
    },
    Once {
        run_at: String,
    },
    Webhook,
    /// A schedule type this build does not know; the row shows and the task cannot be edited.
    Unknown(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStatus {
    Never,
    Running,
    Succeeded,
    Failed,
}

impl RunStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

/// `statusVariant`: the badge's look, with no badge before the first run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    Error,
    Success,
    Info,
    Outline,
}

pub fn badge(status: RunStatus) -> Option<Badge> {
    match status {
        RunStatus::Never => None,
        RunStatus::Failed => Some(Badge::Error),
        RunStatus::Succeeded => Some(Badge::Success),
        RunStatus::Running => Some(Badge::Info),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub enabled: bool,
    pub schedule: Schedule,
    pub project_id: String,
    pub next_run_at: Option<String>,
    pub last_run_status: RunStatus,
    pub last_run_error: Option<String>,
    pub blocked_by_policy: Option<String>,
    /// A `threadMode` this build does not know; the editor sends it back unchanged.
    pub thread_mode: Option<String>,
    pub webhook_url: Option<String>,
    pub has_secret: bool,
    /// The task as the server sent it, for the editor.
    pub raw: Value,
}

impl Task {
    pub fn is_webhook(&self) -> bool {
        self.schedule == Schedule::Webhook
    }
}

fn decode_schedule(value: &Value) -> Option<Schedule> {
    match value.get("type")?.as_str()? {
        "webhook" => Some(Schedule::Webhook),
        "once" => Some(Schedule::Once {
            run_at: value.get("runAt")?.as_str()?.to_owned(),
        }),
        "interval" => Some(Schedule::Interval {
            every_ms: value.get("everyMs")?.as_f64()?,
        }),
        "fixed_time" => Some(Schedule::FixedTime {
            time_of_day: value.get("timeOfDay")?.as_str()?.to_owned(),
            weekdays: value
                .get("weekdays")
                .and_then(Value::as_array)
                .map(|days| days.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default(),
        }),
        other => Some(Schedule::Unknown(other.to_owned())),
    }
}

pub fn decode_task(value: &Value) -> Option<Task> {
    let text = |key: &str| value.get(key).and_then(Value::as_str);
    let optional = |key: &str| text(key).map(str::to_owned);
    Some(Task {
        id: text("id")?.to_owned(),
        title: text("title")?.to_owned(),
        prompt: text("prompt").unwrap_or_default().to_owned(),
        enabled: value.get("enabled")?.as_bool()?,
        schedule: decode_schedule(value.get("schedule")?)?,
        project_id: text("projectId")?.to_owned(),
        next_run_at: optional("nextRunAt"),
        last_run_status: match text("lastRunStatus") {
            Some("running") => RunStatus::Running,
            Some("succeeded") => RunStatus::Succeeded,
            Some("failed") => RunStatus::Failed,
            _ => RunStatus::Never,
        },
        last_run_error: optional("lastRunError"),
        blocked_by_policy: optional("blockedByPolicy"),
        thread_mode: value
            .get("threadMode")
            .filter(|mode| !mode.is_null())
            .map(|mode| {
                mode.as_str()
                    .map_or_else(|| mode.to_string(), str::to_owned)
            }),
        webhook_url: value
            .get("webhook")
            .and_then(|webhook| webhook.get("url")?.as_str())
            .map(str::to_owned),
        has_secret: value
            .get("webhook")
            .and_then(|webhook| webhook.get("hasSecret")?.as_bool())
            .unwrap_or(false),
        raw: value.clone(),
    })
}

/// A list result or stream item. A task with a schedule or thread mode this build does not know
/// still decodes, and the row says so; one that lacks the fields every task has is dropped, as
/// `ForwardCompatibleArray` does.
pub fn decode_tasks(value: &Value) -> Vec<Task> {
    value
        .get("tasks")
        .and_then(Value::as_array)
        .map(|tasks| tasks.iter().filter_map(decode_task).collect())
        .unwrap_or_default()
}

/// `scheduleLabel`.
pub fn schedule_label(schedule: &Schedule, zone: &Zone) -> String {
    match schedule {
        Schedule::Webhook => "On webhook".to_owned(),
        Schedule::Unknown(kind) => format!("Unsupported schedule ({kind})"),
        Schedule::Once { run_at } => match parse_timestamp(run_at) {
            Some(at) => format!("Once, {}", format_date_time_medium(at, zone)),
            None => "Once, Invalid Date".to_owned(),
        },
        Schedule::Interval { every_ms } => {
            let minutes = every_ms / 60_000.;
            if minutes.fract() == 0. {
                format!("Every {minutes} min")
            } else {
                format!("Every {} sec", (every_ms / 1000. + 0.5).floor())
            }
        }
        Schedule::FixedTime {
            time_of_day,
            weekdays,
        } => {
            let days = if weekdays.is_empty() {
                "Daily".to_owned()
            } else if weekdays.len() == 5 && weekdays.iter().all(|day| (1..=5).contains(day)) {
                "Weekdays".to_owned()
            } else {
                weekdays
                    .iter()
                    .map(|day| {
                        usize::try_from(*day)
                            .ok()
                            .and_then(|i| WEEKDAY_LABELS.get(i))
                            .copied()
                            .unwrap_or("undefined")
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!("{days} at {time_of_day}")
        }
    }
}

/// `relativeLabel`: `formatRelativeTime` for the past, a future label for upcoming runs.
pub fn relative_label(value: Option<&str>, now_ms: i64) -> String {
    let Some(at) = value.and_then(parse_timestamp) else {
        return "Not scheduled".to_owned();
    };
    let diff = at - now_ms;
    if diff <= 0 {
        let seconds = -diff / 1000;
        let minutes = seconds / 60;
        return match seconds {
            0..=59 => "just now".to_owned(),
            _ if minutes < 60 => format!("{minutes}m ago"),
            _ if minutes / 60 < 24 => format!("{}h ago", minutes / 60),
            _ => format!("{}d ago", minutes / 60 / 24),
        };
    }
    let minutes = (diff + 59_999) / 60_000;
    if minutes < 2 {
        return "in under a minute".to_owned();
    }
    if minutes < 60 {
        return format!("in {minutes}m");
    }
    let hours = (minutes * 2 + 60) / 120;
    if hours < 24 {
        return format!("in {hours}h");
    }
    format!("in {}d", (hours * 2 + 24) / 48)
}

/// What follows the schedule on a row's second line.
pub fn status_text(task: &Task, now_ms: i64) -> String {
    if let Some(blocked) = &task.blocked_by_policy {
        return blocked.clone();
    }
    if !task.enabled {
        return "Paused".to_owned();
    }
    if task.is_webhook() {
        return "Listening".to_owned();
    }
    match &task.next_run_at {
        Some(next) => format!("Next run {}", relative_label(Some(next), now_ms)),
        None => "Not scheduled".to_owned(),
    }
}

pub fn summary_line(task: &Task, now_ms: i64, zone: &Zone) -> String {
    format!(
        "{} \u{b7} {}",
        schedule_label(&task.schedule, zone),
        status_text(task, now_ms)
    )
}

/// The environments a scope shows tasks for.
pub fn environments_in_scope<'a>(scope: &Scope, data: &'a ScopeData) -> Vec<&'a Environment> {
    match scope {
        Scope::Unavailable(_) => Vec::new(),
        Scope::All => data.environments.iter().collect(),
        Scope::Environment(id) => data.environments.iter().filter(|e| &e.id == id).collect(),
        Scope::Project(ids) => data
            .environments
            .iter()
            .filter(|e| ids.contains(&e.id))
            .collect(),
    }
}

/// `matchesScheduledTaskScope`: for a project scope the task's project must belong to the
/// selected project on that environment.
pub fn matches_scope(
    scope: &Scope,
    project_key: Option<&str>,
    environment_id: &str,
    task: &Task,
    projects: &ProjectsByEnvironment,
) -> bool {
    match scope {
        Scope::Unavailable(_) => false,
        Scope::All => true,
        Scope::Environment(id) => id == environment_id,
        Scope::Project(ids) => {
            ids.iter().any(|id| id == environment_id)
                && project_key
                    .and_then(|key| projects.get(environment_id)?.get(key))
                    .is_some_and(|members| members.contains(&task.project_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_700_000_000_000;

    fn task(schedule: Schedule) -> Task {
        Task {
            id: "t".to_owned(),
            title: "Title".to_owned(),
            prompt: String::new(),
            enabled: true,
            schedule,
            project_id: "p1".to_owned(),
            next_run_at: None,
            last_run_status: RunStatus::Never,
            last_run_error: None,
            blocked_by_policy: None,
            thread_mode: None,
            webhook_url: None,
            has_secret: false,
            raw: Value::Null,
        }
    }

    fn fixed(days: &[i64]) -> Schedule {
        Schedule::FixedTime {
            time_of_day: "09:30".to_owned(),
            weekdays: days.to_vec(),
        }
    }

    #[test]
    fn schedule_labels() {
        let utc = Zone::Utc;
        let label = |s| schedule_label(&s, &utc);
        assert_eq!(label(Schedule::Webhook), "On webhook");
        assert_eq!(
            label(Schedule::Interval { every_ms: 300_000. }),
            "Every 5 min"
        );
        assert_eq!(
            label(Schedule::Interval { every_ms: 90_000. }),
            "Every 90 sec"
        );
        assert_eq!(label(fixed(&[])), "Daily at 09:30");
        assert_eq!(label(fixed(&[1, 2, 3, 4, 5])), "Weekdays at 09:30");
        assert_eq!(label(fixed(&[0, 6])), "Sun, Sat at 09:30");
        assert_eq!(label(fixed(&[1, 2, 3, 4])), "Mon, Tue, Wed, Thu at 09:30");
        assert_eq!(
            label(Schedule::Once {
                run_at: "2026-11-10T05:13:00.000Z".to_owned()
            }),
            "Once, Nov 10, 2026, 5:13\u{202f}AM"
        );
        assert_eq!(
            label(Schedule::Once {
                run_at: "2026-11-10T00:05:00+00:00".to_owned()
            }),
            "Once, Nov 10, 2026, 12:05\u{202f}AM"
        );
    }

    #[test]
    fn relative_labels() {
        let at = |offset: i64| crate::usage_data::iso_from_ms(NOW + offset);
        let label = |offset: i64| relative_label(Some(&at(offset)), NOW);
        assert_eq!(relative_label(None, NOW), "Not scheduled");
        assert_eq!(relative_label(Some("nope"), NOW), "Not scheduled");
        assert_eq!(label(-30_000), "just now");
        assert_eq!(label(0), "just now");
        assert_eq!(label(-5 * 60_000), "5m ago");
        assert_eq!(label(-3 * 3_600_000), "3h ago");
        assert_eq!(label(-3 * 86_400_000), "3d ago");
        assert_eq!(label(30_000), "in under a minute");
        assert_eq!(label(60_000), "in under a minute");
        assert_eq!(label(60_001), "in 2m");
        assert_eq!(label(120_000), "in 2m");
        assert_eq!(label(59 * 60_000), "in 59m");
        assert_eq!(label(90 * 60_000), "in 2h");
        assert_eq!(label(23 * 3_600_000), "in 23h");
        assert_eq!(label(36 * 3_600_000), "in 2d");
        assert_eq!(label(10 * 86_400_000), "in 10d");
    }

    #[test]
    fn status_text_priorities() {
        let mut t = task(Schedule::Interval { every_ms: 60_000. });
        assert_eq!(status_text(&t, NOW), "Not scheduled");
        t.next_run_at = Some(crate::usage_data::iso_from_ms(NOW + 10 * 60_000));
        assert_eq!(status_text(&t, NOW), "Next run in 10m");
        t.enabled = false;
        assert_eq!(status_text(&t, NOW), "Paused");
        t.blocked_by_policy = Some("Blocked by policy".to_owned());
        assert_eq!(status_text(&t, NOW), "Blocked by policy");
        let mut hook = task(Schedule::Webhook);
        assert_eq!(status_text(&hook, NOW), "Listening");
        hook.enabled = false;
        assert_eq!(status_text(&hook, NOW), "Paused");
    }

    #[test]
    fn summary_joins_with_a_middle_dot() {
        let mut t = task(Schedule::Once {
            run_at: "2026-11-10T05:13:00.000Z".to_owned(),
        });
        t.enabled = false;
        assert_eq!(
            summary_line(&t, NOW, &Zone::Utc),
            "Once, Nov 10, 2026, 5:13\u{202f}AM \u{b7} Paused"
        );
    }

    #[test]
    fn badges() {
        assert_eq!(badge(RunStatus::Never), None);
        assert_eq!(badge(RunStatus::Failed), Some(Badge::Error));
        assert_eq!(badge(RunStatus::Succeeded), Some(Badge::Success));
        assert_eq!(badge(RunStatus::Running), Some(Badge::Info));
    }

    #[test]
    fn decoding_keeps_unknown_kinds_and_drops_what_it_cannot_read() {
        let value = json!({ "tasks": [
            {
                "id": "a", "title": "A", "prompt": "do it", "enabled": true,
                "schedule": { "type": "interval", "everyMs": 60000 },
                "projectId": "p1", "nextRunAt": "2026-11-10T05:13:00.000Z",
                "lastRunStatus": "succeeded", "lastRunError": null, "extra": 1
            },
            { "id": "b", "title": "B", "enabled": false,
              "schedule": { "type": "future_kind" }, "projectId": "p1" },
            { "id": "c", "title": "C", "prompt": "", "enabled": true,
              "schedule": { "type": "webhook", "signature": {} }, "projectId": "p2",
              "lastRunStatus": "failed", "lastRunError": "boom", "blockedByPolicy": "No" },
            { "title": "no id" }
        ]});
        let tasks = decode_tasks(&value);
        assert_eq!(tasks.len(), 3);
        assert_eq!(
            tasks[1].schedule,
            Schedule::Unknown("future_kind".to_owned())
        );
        let tasks = [tasks[0].clone(), tasks[2].clone()];
        assert_eq!(tasks[0].last_run_status, RunStatus::Succeeded);
        assert_eq!(tasks[0].last_run_error, None);
        assert_eq!(
            tasks[0].next_run_at.as_deref(),
            Some("2026-11-10T05:13:00.000Z")
        );
        assert!(tasks[1].is_webhook());
        assert_eq!(tasks[1].last_run_error.as_deref(), Some("boom"));
        assert_eq!(tasks[1].blocked_by_policy.as_deref(), Some("No"));
        assert!(decode_tasks(&json!({})).is_empty());
    }

    #[test]
    fn decoding_keeps_a_thread_mode_and_the_webhook_endpoint() {
        let value = json!({ "tasks": [{
            "id": "a", "title": "A", "enabled": true, "projectId": "p",
            "schedule": { "type": "webhook" }, "threadMode": "fresh_session",
            "webhook": { "path": "/h/1", "url": "https://x/h/1", "hasSecret": true }
        }]});
        let task = &decode_tasks(&value)[0];
        assert_eq!(task.thread_mode.as_deref(), Some("fresh_session"));
        assert_eq!(task.webhook_url.as_deref(), Some("https://x/h/1"));
        assert!(task.has_secret);
        assert_eq!(task.raw["id"], "a");
    }

    #[test]
    fn scope_matching() {
        let t = task(Schedule::Webhook);
        let mut projects = ProjectsByEnvironment::new();
        projects
            .entry("primary".to_owned())
            .or_default()
            .insert("repo".to_owned(), vec!["p1".to_owned(), "p9".to_owned()]);
        let check = |scope: &Scope, key: Option<&str>, env: &str| {
            matches_scope(scope, key, env, &t, &projects)
        };
        assert!(check(&Scope::All, None, "primary"));
        assert!(check(&Scope::Environment("a".to_owned()), None, "a"));
        assert!(!check(&Scope::Environment("a".to_owned()), None, "b"));
        assert!(!check(&Scope::Unavailable("gone"), None, "primary"));
        let project = Scope::Project(vec!["primary".to_owned()]);
        assert!(check(&project, Some("repo"), "primary"));
        assert!(!check(&project, Some("other"), "primary"));
        assert!(!check(&project, Some("repo"), "remote"));
        assert!(!check(&project, None, "primary"));
    }
}
