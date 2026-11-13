//! The Scheduled tasks editor's logic (`scheduledTasksSettings.logic.ts`, and the webhook helpers
//! of `packages/client-runtime/src/scheduledTaskWebhook.ts`): the draft, its validation and the
//! `scheduledTasks.upsert` payload it builds.

use crate::usage_data::Zone;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

pub const DEFAULT_WEBHOOK_PROMPT: &str = "Handle this webhook:\n{{body}}";
pub const MAX_WEBHOOK_DELIVERY_AGE_MINUTES: i64 = 24 * 60;
pub const MAX_WEBHOOK_FILTERS: usize = 16;
pub const MIN_INTERVAL_MS: f64 = 60_000.;
pub const SIGNATURE_HEADER: &str = "x-hub-signature-256";
pub const SIGNATURE_PREFIX: &str = "sha256=";

const HOUR_MS: i64 = 3_600_000;

/// Blank means no limit, `Some(None)`; `None` is an input that is not a valid limit, which blocks saving.
pub fn parse_max_delivery_age(value: &str) -> Option<Option<i64>> {
    let value = value.trim();
    if value.is_empty() {
        return Some(None);
    }
    let minutes: f64 = value.parse().ok()?;
    (minutes.is_finite()
        && minutes.fract() == 0.
        && minutes > 0.
        && minutes <= MAX_WEBHOOK_DELIVERY_AGE_MINUTES as f64)
        .then_some(Some(minutes as i64))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOp {
    Equals,
    NotEquals,
    In,
    StartsWith,
    Exists,
}

impl FilterOp {
    /// `WEBHOOK_FILTER_OPS`, in the web's order.
    pub const ALL: [Self; 5] = [
        Self::Equals,
        Self::NotEquals,
        Self::In,
        Self::StartsWith,
        Self::Exists,
    ];

    pub fn wire(self) -> &'static str {
        match self {
            Self::Equals => "equals",
            Self::NotEquals => "notEquals",
            Self::In => "in",
            Self::StartsWith => "startsWith",
            Self::Exists => "exists",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Equals => "equals",
            Self::NotEquals => "does not equal",
            Self::In => "is one of",
            Self::StartsWith => "starts with",
            Self::Exists => "exists",
        }
    }

    pub fn from_wire(wire: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.wire() == wire)
    }
}

/// One editable filter row. `in` takes a comma-separated list; `exists` ignores the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterDraft {
    /// Row identity for the form, never sent.
    pub id: u64,
    pub path: String,
    pub op: FilterOp,
    pub value: String,
}

static LAST_FILTER_ID: AtomicU64 = AtomicU64::new(0);

fn next_filter_id() -> u64 {
    LAST_FILTER_ID.fetch_add(1, Ordering::Relaxed) + 1
}

pub fn empty_filter_draft() -> FilterDraft {
    FilterDraft {
        id: next_filter_id(),
        path: String::new(),
        op: FilterOp::Equals,
        value: String::new(),
    }
}

/// By id, so a removed row's identity never passes to the row after it.
pub fn remove_filter_draft(drafts: &[FilterDraft], id: u64) -> Vec<FilterDraft> {
    drafts.iter().filter(|d| d.id != id).cloned().collect()
}

/// A task's stored `filters`; a row this build cannot read is skipped.
pub fn filter_drafts(filters: Option<&Value>) -> Vec<FilterDraft> {
    filters
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|filter| {
            let op = FilterOp::from_wire(filter.get("op")?.as_str()?)?;
            let value = match op {
                FilterOp::Exists => String::new(),
                FilterOp::In => filter
                    .get("value")?
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => filter.get("value")?.as_str()?.to_owned(),
            };
            Some(FilterDraft {
                id: next_filter_id(),
                path: filter.get("path")?.as_str()?.to_owned(),
                op,
                value,
            })
        })
        .collect()
}

fn valid_filter_path(path: &str) -> bool {
    ["body", "headers", "query"].iter().any(|root| {
        path.strip_prefix(root)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
}

/// Checks each row as the server's schema does; the error names the first row that would not save.
pub fn parse_filter_drafts(drafts: &[FilterDraft]) -> Result<Vec<Value>, String> {
    if drafts.len() > MAX_WEBHOOK_FILTERS {
        return Err(format!("Use at most {MAX_WEBHOOK_FILTERS} filters."));
    }
    let mut filters = Vec::new();
    for (index, draft) in drafts.iter().enumerate() {
        let path = draft.path.trim();
        let filter = if !valid_filter_path(path) {
            None
        } else {
            match draft.op {
                FilterOp::Exists => Some(json!({ "path": path, "op": "exists" })),
                FilterOp::In => {
                    let items: Vec<&str> = draft
                        .value
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .collect();
                    (!items.is_empty()).then(|| json!({ "path": path, "op": "in", "value": items }))
                }
                op => Some(json!({ "path": path, "op": op.wire(), "value": draft.value })),
            }
        };
        match filter {
            Some(filter) => filters.push(filter),
            None => {
                let tail = match draft.op {
                    FilterOp::Exists => ".",
                    FilterOp::In => ", and at least one value.",
                    _ => ", and a value.",
                };
                return Err(format!(
                    "Filter {} needs a path under body, headers or query{tail}",
                    index + 1
                ));
            }
        }
    }
    Ok(filters)
}

/// Whether the rows differ from the ones the form opened with. A save leaves `filters` out unless
/// they did, so editing the title cannot overwrite filters another device changed.
pub fn filter_drafts_changed(saved: &[FilterDraft], current: &[FilterDraft]) -> bool {
    saved.len() != current.len()
        || saved.iter().zip(current).any(|(a, b)| {
            a.path != b.path || a.op != b.op || (a.op != FilterOp::Exists && a.value != b.value)
        })
}

/// A request placeholder at the start of `text`: its length.
fn placeholder_len(text: &[char]) -> Option<usize> {
    if text.get(..2)? != ['{', '{'] {
        return None;
    }
    let mut at = 2;
    let skip_space = |at: &mut usize| {
        while text.get(*at).is_some_and(|c| c.is_whitespace()) {
            *at += 1;
        }
    };
    skip_space(&mut at);
    let word = |at: usize, word: &str| {
        let chars: Vec<char> = word.chars().collect();
        text.get(at..at + chars.len()) == Some(chars.as_slice())
    };
    if word(at, "request") {
        at += 7;
    } else {
        let root = ["body", "headers", "query"]
            .into_iter()
            .find(|root| word(at, root))?;
        at += root.len();
        while text.get(at) == Some(&'.') {
            let start = at + 1;
            let mut end = start;
            while text
                .get(end)
                .is_some_and(|c| !matches!(c, '{' | '}' | '.') && !c.is_whitespace())
            {
                end += 1;
            }
            if end == start {
                break;
            }
            at = end;
        }
    }
    skip_space(&mut at);
    (text.get(at..at + 2)? == ['}', '}']).then_some(at + 2)
}

/// `isWebhookDedupeTemplate`: at least one request placeholder and no stray braces pair.
pub fn is_webhook_dedupe_template(template: &str) -> bool {
    let chars: Vec<char> = template.chars().collect();
    let (mut matches, mut remainder, mut at) = (0, String::new(), 0);
    while at < chars.len() {
        match placeholder_len(&chars[at..]) {
            Some(len) => {
                matches += 1;
                at += len;
            }
            None => {
                remainder.push(chars[at]);
                at += 1;
            }
        }
    }
    matches > 0 && !remainder.contains("{{") && !remainder.contains("}}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fixed,
    Once,
    Interval,
    Webhook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Workspace {
    Root,
    Worktree,
    Existing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Hex,
    Base64,
}

impl Encoding {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Base64 => "base64",
        }
    }
}

pub type Model = (String, String);

#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub editing_id: Option<String>,
    pub title: String,
    pub prompt: String,
    pub enabled: bool,
    pub mode: Mode,
    pub interval_minutes: String,
    pub time_of_day: String,
    /// `YYYY-MM-DDTHH:MM` in the local zone.
    pub run_at: String,
    /// The stored ISO run time, sent back unchanged when the field was not edited.
    pub saved_run_at: String,
    pub weekdays: BTreeSet<i64>,
    pub project_id: String,
    pub thread_id: String,
    pub workspace: Workspace,
    pub base_ref: String,
    pub start_from_origin: bool,
    pub existing_worktree_path: String,
    pub model: Option<Model>,
    pub runtime_mode: String,
    pub interaction_mode: String,
    /// Kept when the picker leaves the instance and model unchanged, so provider options survive.
    pub base_model_selection: Option<Value>,
    /// A thread mode this build does not know, sent back as it came.
    pub thread_mode: Option<Value>,
    pub signature_enabled: bool,
    pub signature_header: String,
    pub signature_encoding: Encoding,
    pub signature_prefix: String,
    /// Write-only: empty keeps the secret already stored.
    pub signature_secret: String,
    pub max_delivery_age_minutes: String,
    pub replay_key: String,
    pub coalesce_key: String,
    pub coalesce_window_seconds: String,
    pub filters: Vec<FilterDraft>,
    pub saved_filters: Vec<FilterDraft>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            editing_id: None,
            title: String::new(),
            prompt: String::new(),
            enabled: true,
            mode: Mode::Fixed,
            interval_minutes: "15".to_owned(),
            time_of_day: "09:00".to_owned(),
            run_at: String::new(),
            saved_run_at: String::new(),
            weekdays: BTreeSet::from([1, 2, 3, 4, 5]),
            project_id: String::new(),
            thread_id: String::new(),
            workspace: Workspace::Worktree,
            base_ref: "main".to_owned(),
            start_from_origin: true,
            existing_worktree_path: String::new(),
            model: None,
            runtime_mode: "full-access".to_owned(),
            interaction_mode: "default".to_owned(),
            base_model_selection: None,
            thread_mode: None,
            signature_enabled: false,
            signature_header: SIGNATURE_HEADER.to_owned(),
            signature_encoding: Encoding::Hex,
            signature_prefix: SIGNATURE_PREFIX.to_owned(),
            signature_secret: String::new(),
            max_delivery_age_minutes: String::new(),
            replay_key: String::new(),
            coalesce_key: String::new(),
            coalesce_window_seconds: String::new(),
            filters: Vec::new(),
            saved_filters: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub title: String,
    pub description: String,
}

fn failure(title: &str, description: impl Into<String>) -> Failure {
    Failure {
        title: title.to_owned(),
        description: description.into(),
    }
}

/// `scheduleDraftErrors`.
pub fn schedule_draft_errors(draft: &Draft, zone: &Zone) -> Vec<Failure> {
    if draft.mode == Mode::Once {
        return match zone.parse_local(&draft.run_at, None) {
            Some(_) => Vec::new(),
            None if zone.local_input_value(0).len() == draft.run_at.len()
                && crate::usage::parse_timestamp(&draft.run_at).is_some() =>
            {
                vec![failure(
                    "Invalid run time",
                    "That time does not exist in your time zone.",
                )]
            }
            None => vec![failure("Invalid run time", "Pick a date and time.")],
        };
    }
    if draft.mode != Mode::Webhook {
        return Vec::new();
    }
    if parse_max_delivery_age(&draft.max_delivery_age_minutes).is_none() {
        return vec![failure(
            "Invalid age limit",
            format!(
                "Enter whole minutes from 1 to {MAX_WEBHOOK_DELIVERY_AGE_MINUTES}, or leave it blank."
            ),
        )];
    }
    let mut errors = Vec::new();
    for (label, value) in [
        ("Duplicate request key", &draft.replay_key),
        ("Coalesce key", &draft.coalesce_key),
    ] {
        let value = value.trim();
        if !value.is_empty() && !is_webhook_dedupe_template(value) {
            errors.push(failure(
                "Invalid webhook settings",
                format!("{label} must contain a valid request placeholder."),
            ));
        }
    }
    let window = draft.coalesce_window_seconds.trim();
    if !window.is_empty() && coalesce_window(window).is_none() {
        errors.push(failure(
            "Invalid webhook settings",
            "Coalesce window must be whole seconds from 10 to 3600.",
        ));
    }
    errors
}

fn coalesce_window(text: &str) -> Option<i64> {
    let seconds: f64 = text.parse().ok()?;
    (seconds.is_finite() && seconds.fract() == 0. && (10. ..=3600.).contains(&seconds))
        .then_some(seconds as i64)
}

/// `scheduleFromDraft`: the upsert schedule, or `None` when the draft has errors.
pub fn schedule_from_draft(draft: &Draft, zone: &Zone) -> Option<Value> {
    if !schedule_draft_errors(draft, zone).is_empty() {
        return None;
    }
    match draft.mode {
        Mode::Once => {
            let saved = crate::usage::parse_timestamp(&draft.saved_run_at);
            if saved.is_some_and(|at| zone.local_input_value(at) == draft.run_at) {
                return Some(json!({ "type": "once", "runAt": draft.saved_run_at }));
            }
            let at = zone.parse_local(&draft.run_at, saved)?;
            Some(json!({ "type": "once", "runAt": crate::usage_data::iso_from_ms(at) }))
        }
        Mode::Webhook => {
            let age = parse_max_delivery_age(&draft.max_delivery_age_minutes)?;
            let replay = draft.replay_key.trim();
            let coalesce = draft.coalesce_key.trim();
            let window = draft.coalesce_window_seconds.trim();
            let window = (!window.is_empty())
                .then(|| coalesce_window(window))
                .flatten();
            let filters = parse_filter_drafts(&draft.filters).ok()?;
            let secret = draft.signature_secret.trim();
            let signature = if draft.signature_enabled {
                let mut signature = json!({
                    "header": draft.signature_header.trim(),
                    "encoding": draft.signature_encoding.wire(),
                    "prefix": draft.signature_prefix,
                });
                if !secret.is_empty() {
                    signature["secret"] = json!(secret);
                }
                signature
            } else {
                Value::Null
            };
            let mut dedupe = Map::new();
            if !replay.is_empty() {
                dedupe.insert("replayKey".to_owned(), json!(replay));
            }
            if !coalesce.is_empty() {
                dedupe.insert("coalesceKey".to_owned(), json!(coalesce));
            }
            if let Some(window) = window {
                dedupe.insert("coalesceWindowSeconds".to_owned(), json!(window));
            }
            let mut schedule = json!({
                "type": "webhook",
                "signature": signature,
                "maxDeliveryAgeMinutes": age,
                "dedupe": dedupe,
            });
            if filter_drafts_changed(&draft.saved_filters, &draft.filters) {
                schedule["filters"] = Value::Array(filters);
            }
            Some(schedule)
        }
        Mode::Interval => {
            let minutes: f64 = draft.interval_minutes.trim().parse().unwrap_or(f64::NAN);
            let every = (minutes * 60_000.).round();
            Some(
                json!({ "type": "interval", "everyMs": if every.is_finite() { json!(every as i64) } else { Value::Null } }),
            )
        }
        Mode::Fixed => {
            let every_day = draft.weekdays.is_empty() || draft.weekdays.len() == 7;
            let time = if draft.time_of_day.is_empty() {
                "09:00"
            } else {
                &draft.time_of_day
            };
            let mut schedule = json!({ "type": "fixed_time", "timeOfDay": time });
            if !every_day {
                schedule["weekdays"] = json!(draft.weekdays.iter().collect::<Vec<_>>());
            }
            Some(schedule)
        }
    }
}

/// The next whole local hour, as a fresh once task's suggested run time.
pub fn next_whole_hour(now_ms: i64, zone: &Zone) -> String {
    let minute: i64 = zone
        .local_input_value(now_ms)
        .get(14..16)
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let start = now_ms - now_ms.rem_euclid(60_000) - minute * 60_000;
    zone.local_input_value(start + HOUR_MS)
}

/// `taskToDraft`, from a task as the server sent it; `None` for a schedule this build cannot edit.
pub fn task_to_draft(task: &Value, zone: &Zone) -> Option<Draft> {
    let text = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    let schedule = task.get("schedule")?;
    let kind = schedule.get("type")?.as_str()?;
    let mut draft = Draft {
        editing_id: Some(text(task, "id")?),
        title: text(task, "title")?,
        prompt: text(task, "prompt").unwrap_or_default(),
        enabled: task.get("enabled")?.as_bool()?,
        project_id: text(task, "projectId")?,
        thread_id: text(task, "threadId").unwrap_or_default(),
        runtime_mode: text(task, "runtimeMode").unwrap_or_else(|| "full-access".to_owned()),
        interaction_mode: text(task, "interactionMode").unwrap_or_else(|| "default".to_owned()),
        thread_mode: task.get("threadMode").filter(|v| !v.is_null()).cloned(),
        ..Draft::default()
    };
    match kind {
        "fixed_time" => {
            draft.mode = Mode::Fixed;
            draft.time_of_day = text(schedule, "timeOfDay")?;
            let days: BTreeSet<i64> = schedule
                .get("weekdays")
                .and_then(Value::as_array)
                .map(|days| days.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            draft.weekdays = if days.is_empty() {
                (0..7).collect()
            } else {
                days
            };
        }
        "once" => {
            draft.mode = Mode::Once;
            let run_at = text(schedule, "runAt")?;
            if let Some(at) = crate::usage::parse_timestamp(&run_at) {
                draft.run_at = zone.local_input_value(at);
            }
            draft.saved_run_at = run_at;
        }
        "interval" => {
            draft.mode = Mode::Interval;
            let minutes = (schedule.get("everyMs")?.as_f64()? / 60_000.).max(1.);
            draft.interval_minutes = minutes.to_string();
        }
        "webhook" => {
            draft.mode = Mode::Webhook;
            draft.filters = filter_drafts(schedule.get("filters"));
            draft.saved_filters = draft.filters.clone();
            if let Some(signature) = schedule.get("signature").filter(|v| v.is_object()) {
                draft.signature_enabled = true;
                draft.signature_header = text(signature, "header").unwrap_or_default();
                draft.signature_encoding =
                    if text(signature, "encoding").as_deref() == Some("base64") {
                        Encoding::Base64
                    } else {
                        Encoding::Hex
                    };
                draft.signature_prefix = text(signature, "prefix").unwrap_or_default();
            }
            let dedupe = schedule.get("dedupe");
            let key = |name: &str| dedupe.and_then(|d| text(d, name)).unwrap_or_default();
            draft.replay_key = key("replayKey");
            draft.coalesce_key = key("coalesceKey");
            draft.coalesce_window_seconds = dedupe
                .and_then(|d| d.get("coalesceWindowSeconds")?.as_i64())
                .map(|seconds| seconds.to_string())
                .unwrap_or_default();
            draft.max_delivery_age_minutes = schedule
                .get("maxDeliveryAgeMinutes")
                .and_then(Value::as_i64)
                .map(|minutes| minutes.to_string())
                .unwrap_or_default();
        }
        _ => return None,
    }
    let strategy = task.get("workspaceStrategy");
    match strategy.and_then(|s| s.get("type")?.as_str()) {
        Some("root") => {
            draft.workspace = Workspace::Root;
        }
        Some("existing_worktree") => {
            draft.workspace = Workspace::Existing;
            draft.existing_worktree_path = strategy
                .and_then(|s| text(s, "worktreePath"))
                .unwrap_or_default();
        }
        _ => {
            draft.workspace = Workspace::Worktree;
            draft.base_ref = strategy
                .and_then(|s| text(s, "baseRef"))
                .unwrap_or_else(|| "main".to_owned());
            draft.start_from_origin = strategy
                .and_then(|s| s.get("startFromOrigin")?.as_bool())
                .unwrap_or(false);
        }
    }
    if let Some(selection) = task.get("modelSelection").filter(|v| v.is_object()) {
        draft.model = text(selection, "instanceId").zip(text(selection, "model"));
        draft.base_model_selection = Some(selection.clone());
    }
    Some(draft)
}

/// A provider instance a task can run on, and its models.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub id: String,
    pub models: Vec<ModelChoice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChoice {
    pub slug: String,
    pub legacy: bool,
    pub default: bool,
}

/// `scheduledTaskDefaultModel` over the instances that are enabled, installed, available and signed
/// in: the configured selections first (unless legacy), then the first default or first model.
pub fn default_model(configured: &[Option<Model>], instances: &[Instance]) -> Option<Model> {
    for (instance_id, model) in configured.iter().flatten() {
        let found = instances.iter().find(|i| &i.id == instance_id);
        if found.is_some_and(|instance| {
            instance
                .models
                .iter()
                .find(|m| &m.slug == model)
                .is_none_or(|m| !m.legacy)
        }) {
            return Some((instance_id.clone(), model.clone()));
        }
    }
    let models: Vec<(&str, &ModelChoice)> = instances
        .iter()
        .flat_map(|instance| {
            instance
                .models
                .iter()
                .filter(|m| !m.legacy)
                .map(|m| (instance.id.as_str(), m))
        })
        .collect();
    let pick = models.iter().find(|(_, m)| m.default).or(models.first())?;
    Some((pick.0.to_owned(), pick.1.slug.clone()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectChoice {
    pub id: String,
    pub pinned: bool,
    pub archived: bool,
}

/// `defaultScheduledTaskProjectId`: a pinned open project, then any open one; an archived one only
/// when Settings is scoped to a project, so that scope can still save.
pub fn default_project_id(projects: &[ProjectChoice], scoped_to_project: bool) -> String {
    let mut open = projects.iter().filter(|p| !p.archived);
    let pinned = projects.iter().find(|p| !p.archived && p.pinned);
    pinned
        .or_else(|| open.next())
        .or_else(|| scoped_to_project.then(|| projects.first()).flatten())
        .map(|p| p.id.clone())
        .unwrap_or_default()
}

/// What the save needs from outside the draft.
pub struct Context<'a> {
    pub project_ids: &'a [String],
    pub zone: &'a Zone,
    pub offers_once: bool,
    /// The task being edited already stores a webhook secret.
    pub has_secret: bool,
    /// The model the form shows: the draft's pick, else the default.
    pub selection: Option<Model>,
}

/// The submit checks and payload of `ScheduledTasksSettings.tsx`.
pub fn build_input(draft: &Draft, ctx: &Context) -> Result<Value, Failure> {
    let incomplete = || {
        failure(
            "Scheduled task is incomplete",
            "Add a title, prompt, project, and model.",
        )
    };
    let selection = ctx.selection.clone().ok_or_else(incomplete)?;
    if draft.title.trim().is_empty()
        || draft.prompt.trim().is_empty()
        || !ctx.project_ids.contains(&draft.project_id)
    {
        return Err(incomplete());
    }
    if draft.mode == Mode::Webhook
        && let Err(error) = parse_filter_drafts(&draft.filters)
    {
        return Err(failure("Invalid filter", error));
    }
    let Some(schedule) = schedule_from_draft(draft, ctx.zone) else {
        let first = schedule_draft_errors(draft, ctx.zone).into_iter().next();
        return Err(first.unwrap_or_else(|| failure("Invalid schedule", "Check the schedule.")));
    };
    if let Some(signature) = schedule.get("signature").filter(|v| v.is_object()) {
        let header_empty = signature["header"].as_str().is_none_or(str::is_empty);
        if header_empty || (signature.get("secret").is_none() && !ctx.has_secret) {
            return Err(failure(
                "Signing secret is required",
                "Enter the signature header and secret.",
            ));
        }
    }
    if draft.mode == Mode::Interval {
        let every = schedule["everyMs"].as_f64().unwrap_or(f64::NAN);
        if !every.is_finite() || every < MIN_INTERVAL_MS {
            return Err(failure(
                "Invalid interval",
                "Enter an interval of at least one minute.",
            ));
        }
    }
    if draft.mode == Mode::Once && !ctx.offers_once {
        return Err(failure(
            "Once is not available here",
            "This environment runs an older T3 Code. Update it to schedule a one-time task, or pick another schedule.",
        ));
    }
    if draft.workspace == Workspace::Existing && draft.existing_worktree_path.trim().is_empty() {
        return Err(failure(
            "Checkout path is required",
            "Enter the path of the checkout to run in.",
        ));
    }
    let model_selection = match &draft.base_model_selection {
        Some(base)
            if base["instanceId"].as_str() == Some(selection.0.as_str())
                && base["model"].as_str() == Some(selection.1.as_str()) =>
        {
            base.clone()
        }
        _ => json!({ "instanceId": selection.0, "model": selection.1 }),
    };
    let workspace_strategy = match draft.workspace {
        Workspace::Root => json!({ "type": "root" }),
        Workspace::Existing => {
            json!({ "type": "existing_worktree", "worktreePath": draft.existing_worktree_path.trim() })
        }
        Workspace::Worktree => {
            let base = draft.base_ref.trim();
            json!({
                "type": "worktree",
                "baseRef": if base.is_empty() { "main" } else { base },
                "startFromOrigin": draft.start_from_origin,
            })
        }
    };
    let mut input = json!({
        "title": draft.title.trim(),
        "prompt": draft.prompt.trim(),
        "enabled": draft.enabled,
        "schedule": schedule,
        "projectId": draft.project_id,
        "threadId": if draft.thread_id.is_empty() { Value::Null } else { json!(draft.thread_id) },
        "workspaceStrategy": workspace_strategy,
        "modelSelection": model_selection,
        "runtimeMode": draft.runtime_mode,
        "interactionMode": draft.interaction_mode,
        "creationSource": "web",
    });
    if let Some(id) = &draft.editing_id {
        input["id"] = json!(id);
        input["requireExisting"] = json!(true);
    }
    if let Some(mode) = &draft.thread_mode {
        input["threadMode"] = mode.clone();
    }
    Ok(input)
}

/// What the editor offers, fed from the attached environment and the other connected ones.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorData {
    pub projects: std::collections::HashMap<String, Vec<ProjectOption>>,
    pub models: Vec<ModelGroup>,
    pub default_model: Option<Model>,
    pub threads: Vec<ThreadOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectOption {
    pub id: String,
    pub title: String,
    pub pinned: bool,
    pub archived: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelGroup {
    pub instance_id: String,
    pub name: String,
    pub models: Vec<(String, String, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadOption {
    pub id: String,
    pub project_id: String,
    pub title: String,
}

impl EditorData {
    pub fn instances(&self) -> Vec<Instance> {
        self.models
            .iter()
            .map(|group| Instance {
                id: group.instance_id.clone(),
                models: group
                    .models
                    .iter()
                    .map(|(slug, _, legacy)| ModelChoice {
                        slug: slug.clone(),
                        legacy: *legacy,
                        default: false,
                    })
                    .collect(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc() -> Zone {
        Zone::Utc
    }

    fn filter(path: &str, op: FilterOp, value: &str) -> FilterDraft {
        FilterDraft {
            id: next_filter_id(),
            path: path.to_owned(),
            op,
            value: value.to_owned(),
        }
    }

    #[test]
    fn delivery_age_blank_valid_and_invalid() {
        assert_eq!(parse_max_delivery_age("  "), Some(None));
        assert_eq!(parse_max_delivery_age(" 30 "), Some(Some(30)));
        assert_eq!(parse_max_delivery_age("1440"), Some(Some(1440)));
        assert_eq!(parse_max_delivery_age("1441"), None);
        assert_eq!(parse_max_delivery_age("0"), None);
        assert_eq!(parse_max_delivery_age("1.5"), None);
        assert_eq!(parse_max_delivery_age("abc"), None);
    }

    #[test]
    fn dedupe_templates() {
        assert!(is_webhook_dedupe_template("{{headers.delivery-id}}"));
        assert!(is_webhook_dedupe_template("a-{{ body.id }}-{{request}}"));
        assert!(is_webhook_dedupe_template("{{body}}"));
        assert!(!is_webhook_dedupe_template("plain"));
        assert!(!is_webhook_dedupe_template("{{foo.bar}}"));
        assert!(!is_webhook_dedupe_template("{{body.id}} {{"));
        assert!(!is_webhook_dedupe_template("{{body.id}}}}"));
        assert!(!is_webhook_dedupe_template("{{body.}}"));
        assert!(!is_webhook_dedupe_template(""));
    }

    #[test]
    fn filter_rows_parse_like_the_schema() {
        let rows = [
            filter(" body.action ", FilterOp::Equals, "opened"),
            filter("headers.x-event", FilterOp::In, " a, b ,, c "),
            filter("query", FilterOp::Exists, "ignored"),
        ];
        let parsed = parse_filter_drafts(&rows).unwrap();
        assert_eq!(
            parsed,
            vec![
                json!({"path": "body.action", "op": "equals", "value": "opened"}),
                json!({"path": "headers.x-event", "op": "in", "value": ["a", "b", "c"]}),
                json!({"path": "query", "op": "exists"}),
            ]
        );
        let bad_path = [filter("bodyx", FilterOp::Equals, "")];
        assert_eq!(
            parse_filter_drafts(&bad_path).unwrap_err(),
            "Filter 1 needs a path under body, headers or query, and a value."
        );
        let empty_in = [
            filter("body.a", FilterOp::Equals, ""),
            filter("body.a", FilterOp::In, " , "),
        ];
        assert_eq!(
            parse_filter_drafts(&empty_in).unwrap_err(),
            "Filter 2 needs a path under body, headers or query, and at least one value."
        );
        let too_many: Vec<_> = (0..17).map(|_| empty_filter_draft()).collect();
        assert_eq!(
            parse_filter_drafts(&too_many).unwrap_err(),
            "Use at most 16 filters."
        );
    }

    #[test]
    fn filter_drafts_round_trip_and_compare() {
        let stored = json!([
            {"path": "body.a", "op": "in", "value": ["x", "y"]},
            {"path": "body.b", "op": "exists"},
            {"path": "body.c", "op": "startsWith", "value": "z"},
            {"path": "body.d", "op": "future"},
        ]);
        let drafts = filter_drafts(Some(&stored));
        assert_eq!(drafts.len(), 3);
        assert_eq!(drafts[0].value, "x, y");
        assert_eq!(drafts[1].value, "");
        assert!(!filter_drafts_changed(&drafts, &drafts.clone()));
        let mut edited = drafts.clone();
        edited[1].value = "ignored".to_owned();
        assert!(!filter_drafts_changed(&drafts, &edited));
        edited[0].value = "x".to_owned();
        assert!(filter_drafts_changed(&drafts, &edited));
        assert!(filter_drafts_changed(&drafts, &drafts[..2]));
        let ids = remove_filter_draft(&drafts, drafts[1].id);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[1].path, "body.c");
        assert_ne!(empty_filter_draft().id, empty_filter_draft().id);
        assert!(filter_drafts(None).is_empty());
    }

    #[test]
    fn fixed_schedule_sorts_and_omits_every_day() {
        let mut draft = Draft {
            weekdays: BTreeSet::from([5, 0, 3]),
            ..Draft::default()
        };
        assert_eq!(
            schedule_from_draft(&draft, &utc()),
            Some(json!({"type": "fixed_time", "timeOfDay": "09:00", "weekdays": [0, 3, 5]}))
        );
        draft.weekdays = (0..7).collect();
        draft.time_of_day = String::new();
        assert_eq!(
            schedule_from_draft(&draft, &utc()),
            Some(json!({"type": "fixed_time", "timeOfDay": "09:00"}))
        );
        draft.weekdays.clear();
        assert!(
            schedule_from_draft(&draft, &utc())
                .unwrap()
                .get("weekdays")
                .is_none()
        );
    }

    #[test]
    fn interval_schedule_rounds_to_milliseconds() {
        let mut draft = Draft {
            mode: Mode::Interval,
            interval_minutes: "1.5".to_owned(),
            ..Draft::default()
        };
        assert_eq!(
            schedule_from_draft(&draft, &utc()),
            Some(json!({"type": "interval", "everyMs": 90000}))
        );
        draft.interval_minutes = "x".to_owned();
        assert_eq!(
            schedule_from_draft(&draft, &utc()),
            Some(json!({"type": "interval", "everyMs": null}))
        );
    }

    #[test]
    fn once_schedule_keeps_saved_time_until_edited() {
        let zone = utc();
        let saved = "2026-11-10T05:13:45.000Z";
        let mut draft = Draft {
            mode: Mode::Once,
            run_at: "2026-11-10T05:13".to_owned(),
            saved_run_at: saved.to_owned(),
            ..Draft::default()
        };
        assert_eq!(
            schedule_from_draft(&draft, &zone),
            Some(json!({"type": "once", "runAt": saved}))
        );
        draft.run_at = "2026-11-11T06:00".to_owned();
        assert_eq!(
            schedule_from_draft(&draft, &zone),
            Some(json!({"type": "once", "runAt": "2026-11-11T06:00:00.000Z"}))
        );
    }

    #[test]
    fn once_errors() {
        let zone = utc();
        let mut draft = Draft {
            mode: Mode::Once,
            ..Draft::default()
        };
        assert_eq!(
            schedule_draft_errors(&draft, &zone)[0].description,
            "Pick a date and time."
        );
        draft.run_at = "2026-02-31T10:00".to_owned();
        assert!(schedule_draft_errors(&draft, &zone).len() == 1);
        assert!(schedule_from_draft(&draft, &zone).is_none());
        draft.run_at = "2026-02-27T10:00".to_owned();
        assert!(schedule_draft_errors(&draft, &zone).is_empty());
    }

    #[test]
    fn local_time_skips_and_repeats() {
        let zone = Zone::Local("America/New_York".to_owned());
        let before = std::env::var("TZ").ok();
        // SAFETY: tests that read the zone run with the same value, and nothing else sets TZ.
        unsafe { std::env::set_var("TZ", "America/New_York") };
        let skipped = zone.parse_local("2026-03-08T02:30", None);
        let repeated_first = zone.parse_local("2026-11-01T01:30", None).unwrap();
        let repeated_second = zone.parse_local("2026-11-01T01:30", Some(repeated_first + HOUR_MS));
        let plain = zone.parse_local("2026-07-01T12:00", None);
        match before {
            // SAFETY: as above.
            Some(value) => unsafe { std::env::set_var("TZ", value) },
            None => unsafe { std::env::remove_var("TZ") },
        }
        assert_eq!(skipped, None);
        assert_eq!(repeated_second, Some(repeated_first + HOUR_MS));
        assert_eq!(plain, crate::usage::parse_timestamp("2026-07-01T16:00:00Z"));
    }

    #[test]
    fn next_hour_is_whole() {
        let at = crate::usage::parse_timestamp("2026-11-10T05:13:45Z").unwrap();
        assert_eq!(next_whole_hour(at, &utc()), "2026-11-10T06:00");
        let on_hour = crate::usage::parse_timestamp("2026-11-10T23:00:00Z").unwrap();
        assert_eq!(next_whole_hour(on_hour, &utc()), "2026-11-11T00:00");
    }

    #[test]
    fn webhook_schedule() {
        let mut draft = Draft {
            mode: Mode::Webhook,
            ..Draft::default()
        };
        assert_eq!(
            schedule_from_draft(&draft, &utc()),
            Some(json!({
                "type": "webhook", "signature": null,
                "maxDeliveryAgeMinutes": null, "dedupe": {}
            }))
        );
        draft.signature_enabled = true;
        draft.signature_secret = " s3 ".to_owned();
        draft.max_delivery_age_minutes = "60".to_owned();
        draft.replay_key = " {{headers.id}} ".to_owned();
        draft.coalesce_window_seconds = "120".to_owned();
        draft.filters = vec![filter("body.a", FilterOp::Equals, "b")];
        assert_eq!(
            schedule_from_draft(&draft, &utc()),
            Some(json!({
                "type": "webhook",
                "signature": {
                    "header": "x-hub-signature-256", "encoding": "hex",
                    "prefix": "sha256=", "secret": "s3"
                },
                "maxDeliveryAgeMinutes": 60,
                "dedupe": {"replayKey": "{{headers.id}}", "coalesceWindowSeconds": 120},
                "filters": [{"path": "body.a", "op": "equals", "value": "b"}],
            }))
        );
        draft.saved_filters = draft.filters.clone();
        assert!(
            schedule_from_draft(&draft, &utc())
                .unwrap()
                .get("filters")
                .is_none()
        );
    }

    #[test]
    fn webhook_errors() {
        let mut draft = Draft {
            mode: Mode::Webhook,
            max_delivery_age_minutes: "0".to_owned(),
            ..Draft::default()
        };
        assert_eq!(
            schedule_draft_errors(&draft, &utc())[0].title,
            "Invalid age limit"
        );
        draft.max_delivery_age_minutes.clear();
        draft.replay_key = "nope".to_owned();
        draft.coalesce_key = "{{body.id}}".to_owned();
        draft.coalesce_window_seconds = "5".to_owned();
        let errors = schedule_draft_errors(&draft, &utc());
        assert_eq!(errors.len(), 2);
        assert_eq!(
            errors[0].description,
            "Duplicate request key must contain a valid request placeholder."
        );
        assert_eq!(
            errors[1].description,
            "Coalesce window must be whole seconds from 10 to 3600."
        );
        assert!(schedule_from_draft(&draft, &utc()).is_none());
    }

    #[test]
    fn task_to_draft_reads_every_kind() {
        let base = |schedule: Value| {
            json!({
                "id": "t1", "title": "T", "prompt": "P", "enabled": false,
                "projectId": "p", "threadId": "th",
                "schedule": schedule,
                "workspaceStrategy": {"type": "existing_worktree", "worktreePath": "/w"},
                "modelSelection": {"instanceId": "codex", "model": "m", "options": []},
                "runtimeMode": "approval-required", "interactionMode": "plan",
                "threadMode": "fresh_session",
            })
        };
        let fixed = task_to_draft(
            &base(json!({"type": "fixed_time", "timeOfDay": "07:15", "weekdays": [1, 2]})),
            &utc(),
        )
        .unwrap();
        assert_eq!(fixed.weekdays, BTreeSet::from([1, 2]));
        assert_eq!(fixed.time_of_day, "07:15");
        assert_eq!(fixed.workspace, Workspace::Existing);
        assert_eq!(fixed.existing_worktree_path, "/w");
        assert_eq!(fixed.model, Some(("codex".to_owned(), "m".to_owned())));
        assert_eq!(fixed.thread_id, "th");
        assert_eq!(fixed.thread_mode, Some(json!("fresh_session")));
        assert_eq!(fixed.runtime_mode, "approval-required");
        assert!(!fixed.enabled);

        let daily = task_to_draft(
            &base(json!({"type": "fixed_time", "timeOfDay": "07:15"})),
            &utc(),
        )
        .unwrap();
        assert_eq!(daily.weekdays.len(), 7);

        let interval =
            task_to_draft(&base(json!({"type": "interval", "everyMs": 30000})), &utc()).unwrap();
        assert_eq!(interval.interval_minutes, "1");
        let interval = task_to_draft(
            &base(json!({"type": "interval", "everyMs": 150000})),
            &utc(),
        )
        .unwrap();
        assert_eq!(interval.interval_minutes, "2.5");

        let once = task_to_draft(
            &base(json!({"type": "once", "runAt": "2026-11-10T05:13:00.000Z"})),
            &utc(),
        )
        .unwrap();
        assert_eq!(once.run_at, "2026-11-10T05:13");
        assert_eq!(once.saved_run_at, "2026-11-10T05:13:00.000Z");

        let hook = task_to_draft(
            &base(json!({
                "type": "webhook",
                "signature": {"header": "x-sig", "encoding": "base64", "prefix": ""},
                "maxDeliveryAgeMinutes": 10,
                "dedupe": {"replayKey": "{{body.id}}", "coalesceWindowSeconds": 30},
                "filters": [{"path": "body.a", "op": "exists"}],
            })),
            &utc(),
        )
        .unwrap();
        assert!(hook.signature_enabled);
        assert_eq!(hook.signature_encoding, Encoding::Base64);
        assert_eq!(hook.signature_prefix, "");
        assert_eq!(hook.max_delivery_age_minutes, "10");
        assert_eq!(hook.coalesce_window_seconds, "30");
        assert_eq!(hook.replay_key, "{{body.id}}");
        assert_eq!(hook.filters, hook.saved_filters);
        let off =
            task_to_draft(&base(json!({"type": "webhook", "signature": null})), &utc()).unwrap();
        assert!(!off.signature_enabled);
        assert_eq!(off.signature_header, SIGNATURE_HEADER);

        assert!(task_to_draft(&base(json!({"type": "future"})), &utc()).is_none());
    }

    fn instance(id: &str, models: &[(&str, bool, bool)]) -> Instance {
        Instance {
            id: id.to_owned(),
            models: models
                .iter()
                .map(|(slug, legacy, default)| ModelChoice {
                    slug: (*slug).to_owned(),
                    legacy: *legacy,
                    default: *default,
                })
                .collect(),
        }
    }

    #[test]
    fn default_model_prefers_configured_then_default_then_first() {
        let instances = [
            instance("a", &[("old", true, false), ("new", false, false)]),
            instance("b", &[("x", false, false), ("y", false, true)]),
        ];
        let pick = |m: &str, i: &str| Some((i.to_owned(), m.to_owned()));
        assert_eq!(default_model(&[pick("y", "b")], &instances), pick("y", "b"));
        assert_eq!(
            default_model(&[pick("old", "a")], &instances),
            pick("y", "b")
        );
        assert_eq!(
            default_model(&[pick("x", "gone")], &instances),
            pick("y", "b")
        );
        assert_eq!(
            default_model(&[None, pick("x", "b")], &instances),
            pick("x", "b")
        );
        assert_eq!(default_model(&[], &instances[..1]), pick("new", "a"));
        assert_eq!(default_model(&[], &[]), None);
    }

    #[test]
    fn default_project_prefers_pinned_open() {
        let project = |id: &str, pinned: bool, archived: bool| ProjectChoice {
            id: id.to_owned(),
            pinned,
            archived,
        };
        let list = [
            project("a", false, false),
            project("b", true, false),
            project("c", true, true),
        ];
        assert_eq!(default_project_id(&list, false), "b");
        assert_eq!(default_project_id(&list[..1], false), "a");
        let archived = [project("c", true, true)];
        assert_eq!(default_project_id(&archived, false), "");
        assert_eq!(default_project_id(&archived, true), "c");
        assert_eq!(default_project_id(&[], true), "");
    }

    fn ready() -> (Draft, Vec<String>) {
        let draft = Draft {
            title: " T ".to_owned(),
            prompt: " P ".to_owned(),
            project_id: "p".to_owned(),
            ..Draft::default()
        };
        (draft, vec!["p".to_owned()])
    }

    fn context<'a>(ids: &'a [String], zone: &'a Zone) -> Context<'a> {
        Context {
            project_ids: ids,
            zone,
            offers_once: true,
            has_secret: false,
            selection: Some(("codex".to_owned(), "m".to_owned())),
        }
    }

    #[test]
    fn input_for_a_new_task() {
        let (draft, ids) = ready();
        let zone = utc();
        let input = build_input(&draft, &context(&ids, &zone)).unwrap();
        assert_eq!(input["title"], "T");
        assert_eq!(input["prompt"], "P");
        assert_eq!(input["threadId"], Value::Null);
        assert_eq!(
            input["workspaceStrategy"],
            json!({"type": "worktree", "baseRef": "main", "startFromOrigin": true})
        );
        assert_eq!(
            input["modelSelection"],
            json!({"instanceId": "codex", "model": "m"})
        );
        assert!(input.get("id").is_none());
        assert!(input.get("threadMode").is_none());
    }

    #[test]
    fn input_for_an_edit_keeps_options_and_unknown_mode() {
        let (mut draft, ids) = ready();
        let zone = utc();
        draft.editing_id = Some("t1".to_owned());
        draft.thread_mode = Some(json!("fresh_session"));
        draft.base_model_selection = Some(
            json!({"instanceId": "codex", "model": "m", "options": [{"id": "e", "value": "hi"}]}),
        );
        let input = build_input(&draft, &context(&ids, &zone)).unwrap();
        assert_eq!(input["id"], "t1");
        assert_eq!(input["requireExisting"], true);
        assert_eq!(input["threadMode"], "fresh_session");
        assert_eq!(input["modelSelection"]["options"][0]["value"], "hi");
        let mut ctx = context(&ids, &zone);
        ctx.selection = Some(("codex".to_owned(), "other".to_owned()));
        let input = build_input(&draft, &ctx).unwrap();
        assert_eq!(
            input["modelSelection"],
            json!({"instanceId": "codex", "model": "other"})
        );
    }

    #[test]
    fn submit_checks() {
        let (mut draft, ids) = ready();
        let zone = utc();
        let title = |draft: &Draft, ctx: &Context| build_input(draft, ctx).unwrap_err().title;
        let mut ctx = context(&ids, &zone);
        ctx.selection = None;
        assert_eq!(title(&draft, &ctx), "Scheduled task is incomplete");
        let ctx = context(&ids, &zone);
        draft.project_id = "other".to_owned();
        assert_eq!(title(&draft, &ctx), "Scheduled task is incomplete");
        draft.project_id = "p".to_owned();
        draft.workspace = Workspace::Existing;
        assert_eq!(title(&draft, &ctx), "Checkout path is required");
        draft.workspace = Workspace::Root;
        draft.mode = Mode::Interval;
        draft.interval_minutes = "0.5".to_owned();
        assert_eq!(title(&draft, &ctx), "Invalid interval");
        draft.mode = Mode::Once;
        draft.run_at = "2026-11-10T05:00".to_owned();
        let mut no_once = context(&ids, &zone);
        no_once.offers_once = false;
        assert_eq!(title(&draft, &no_once), "Once is not available here");
        draft.mode = Mode::Webhook;
        draft.signature_enabled = true;
        assert_eq!(title(&draft, &ctx), "Signing secret is required");
        let mut stored = context(&ids, &zone);
        stored.has_secret = true;
        assert!(build_input(&draft, &stored).is_ok());
        draft.filters = vec![filter("nope", FilterOp::Equals, "x")];
        assert_eq!(title(&draft, &stored), "Invalid filter");
    }
}
