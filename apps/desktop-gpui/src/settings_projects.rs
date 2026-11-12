//! The Projects tab's model (`ProjectSettingsPanel.tsx`, `ProjectSettingsPanel.logic.ts` and
//! `planProjectRemoval`): the `projects.mutate` payloads a row sends and the copy the Danger
//! section and its confirmation show.

use crate::projects::Member;
use serde_json::{Value, json};

/// Call tokens, above every other range.
pub const TOKEN_BASE: u64 = 1 << 52;

/// One `project.update`: `fields` are the keys of `ProjectUpdatePayload` that change.
pub fn update_payload(project_id: &str, command_id: String, fields: Value) -> Value {
    let mut payload = json!({
        "type": "project.update",
        "commandId": command_id,
        "projectId": project_id,
    });
    if let (Some(target), Some(source)) = (payload.as_object_mut(), fields.as_object()) {
        target.extend(source.clone());
    }
    payload
}

/// `project.delete` with `force`, as the web's remove sends it.
pub fn delete_payload(project_id: &str, command_id: String) -> Value {
    json!({
        "type": "project.delete",
        "commandId": command_id,
        "projectId": project_id,
        "force": true,
    })
}

/// `projectGroupTitleNeedsUpdate` with the trim the input applies: the title to write, when the
/// edit changes any member.
pub fn title_to_write(members: &[Member], edited: &str) -> Result<Option<String>, &'static str> {
    let title = edited.trim();
    if title.is_empty() {
        return Err("Project title cannot be empty");
    }
    Ok(members
        .iter()
        .any(|member| member.title != title)
        .then(|| title.to_owned()))
}

/// What the Danger section says for the checkouts it removes.
pub struct Danger {
    pub title: &'static str,
    pub description: String,
    pub button: &'static str,
}

/// The Danger section's copy: `hasOtherMembers` and the number of entries selected.
pub fn danger(has_other_members: bool, members: usize) -> Danger {
    if has_other_members {
        Danger {
            title: "Remove checkout",
            description: "Deletes the selected machine's checkout entries and their threads. Other machines and files on disk are not touched.".to_owned(),
            button: "Remove checkout",
        }
    } else if members > 1 {
        Danger {
            title: "Remove this project everywhere",
            description: format!(
                "Deletes all {members} checkout entries and their threads on every machine. Files on disk are not touched."
            ),
            button: "Remove all entries",
        }
    } else {
        Danger {
            title: "Remove project",
            description:
                "Deletes the project entry and its threads. Files on disk are not touched."
                    .to_owned(),
            button: "Remove project",
        }
    }
}

/// `planProjectRemoval`'s confirmation message, one line per entry of the array.
pub fn confirmation(
    members: &[Member],
    group_member_count: usize,
    has_other_members: bool,
    display_name: &str,
    environment_label: Option<&str>,
    thread_count: usize,
) -> Vec<String> {
    let whole_group = members.len() == group_member_count;
    let kind = if has_other_members || !whole_group {
        "checkout"
    } else {
        "project"
    };
    let single = (members.len() == 1).then(|| &members[0]);
    let label = single.map_or(display_name, |member| member.title.as_str());
    let mut lines = vec![if thread_count > 0 {
        format!(
            "Remove {kind} \"{label}\" and delete its {thread_count} thread{}?",
            if thread_count == 1 { "" } else { "s" }
        )
    } else {
        format!("Remove {kind} \"{label}\"?")
    }];
    match single {
        Some(member) => {
            lines.push(format!("Path: {}", member.root));
            if let Some(environment) = environment_label.filter(|label| !label.is_empty()) {
                lines.push(format!("Environment: {environment}"));
            }
        }
        None => lines.push(format!(
            "This removes {} grouped project entries.",
            members.len()
        )),
    }
    lines.push(
        if thread_count > 0 {
            "Those threads and any archived threads are deleted from T3 Code. Provider sessions are kept."
        } else {
            "Any archived threads are deleted from T3 Code. Provider sessions are kept."
        }
        .to_owned(),
    );
    lines.push(
        if whole_group && !has_other_members {
            "This removes only the project entries, not the files on disk."
        } else {
            "Other entries in this grouped project are unaffected."
        }
        .to_owned(),
    );
    lines.push("This action cannot be undone.".to_owned());
    lines
}

/// The description of the Project icon row: the override, else the favicon path, else Automatic.
pub fn icon_description(member: &Member) -> String {
    member
        .icon
        .clone()
        .or_else(|| member.favicon_path.clone())
        .unwrap_or_else(|| "Automatic".to_owned())
}

/// `hasDefaultProjectIcon` for a project that is not the scratch one.
pub fn has_default_icon(member: &Member) -> bool {
    member.favicon_path.is_none() && member.icon.is_none()
}

/// `MAX_SCRIPT_ID_LENGTH`.
const SCRIPT_ID_MAX: usize = 24;

/// The action icons (`ProjectScriptIcon`), in the editor's order, with their labels.
pub const SCRIPT_ICONS: [(&str, &str); 6] = [
    ("play", "Play"),
    ("test", "Test"),
    ("lint", "Lint"),
    ("configure", "Configure"),
    ("build", "Build"),
    ("debug", "Debug"),
];

/// A project action (`ProjectScript`): the editor's fields, in the shape the form holds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub id: String,
    pub name: String,
    pub command: String,
    pub icon: String,
    pub run_on_worktree_create: bool,
    /// `async: false`: the agent waits for the setup to finish.
    pub wait_for_setup: bool,
    pub run_on_settle: bool,
    pub stop_on_settle: bool,
    /// Empty when the action has no preview.
    pub preview_url: String,
    pub auto_open_preview: bool,
}

impl Script {
    /// A blank form for a new action.
    pub fn blank() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            command: String::new(),
            icon: "play".to_owned(),
            run_on_worktree_create: false,
            wait_for_setup: false,
            run_on_settle: false,
            stop_on_settle: false,
            preview_url: String::new(),
            auto_open_preview: false,
        }
    }

    /// Reads one entry of `defaultProjectScripts`; an entry without an id, name or command is skipped.
    pub fn decode(value: &Value) -> Option<Self> {
        let text = |key: &str| value.get(key).and_then(Value::as_str);
        let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
        let icon = text("icon").unwrap_or("play");
        Some(Self {
            id: text("id")?.to_owned(),
            name: text("name")?.to_owned(),
            command: text("command")?.to_owned(),
            icon: if SCRIPT_ICONS.iter().any(|(known, _)| *known == icon) {
                icon.to_owned()
            } else {
                "play".to_owned()
            },
            run_on_worktree_create: flag("runOnWorktreeCreate"),
            wait_for_setup: value.get("async").and_then(Value::as_bool) == Some(false),
            run_on_settle: flag("runOnSettle"),
            stop_on_settle: flag("stopOnSettle"),
            preview_url: text("previewUrl").unwrap_or_default().to_owned(),
            auto_open_preview: flag("autoOpenPreview"),
        })
    }

    /// `buildProjectScript`: optional keys appear only when they matter.
    pub fn encode(&self) -> Value {
        let mut script = json!({
            "id": self.id,
            "name": self.name,
            "command": self.command,
            "icon": self.icon,
            "runOnWorktreeCreate": self.run_on_worktree_create,
        });
        if self.run_on_worktree_create && self.wait_for_setup {
            script["async"] = json!(false);
        }
        if self.run_on_settle {
            script["runOnSettle"] = json!(true);
        }
        if self.stop_on_settle {
            script["stopOnSettle"] = json!(true);
        }
        if !self.preview_url.is_empty() {
            script["previewUrl"] = json!(self.preview_url);
            script["autoOpenPreview"] = json!(self.auto_open_preview);
        }
        script
    }
}

pub fn decode_scripts(value: &Value) -> Vec<Script> {
    value
        .as_array()
        .map(|scripts| scripts.iter().filter_map(Script::decode).collect())
        .unwrap_or_default()
}

/// The editor's validation, in the order it shows them.
pub fn script_problem(name: &str, command: &str) -> Option<&'static str> {
    if name.trim().is_empty() {
        Some("Name is required.")
    } else if command.trim().is_empty() {
        Some("Command is required.")
    } else {
        None
    }
}

fn normalized_script_id(name: &str) -> String {
    let mut cleaned = String::new();
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            cleaned.push(ch);
        } else if !cleaned.ends_with('-') {
            cleaned.push('-');
        }
    }
    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() {
        return "script".to_owned();
    }
    if cleaned.len() <= SCRIPT_ID_MAX {
        return cleaned.to_owned();
    }
    let cut = cleaned[..SCRIPT_ID_MAX].trim_end_matches('-');
    if cut.is_empty() { "script" } else { cut }.to_owned()
}

/// `nextProjectScriptId`: the name's slug, then `-2`, `-3` and so on while it is taken.
pub fn next_script_id(name: &str, existing: &[String]) -> String {
    let base = normalized_script_id(name);
    if !existing.contains(&base) {
        return base;
    }
    for suffix in 2..10_000 {
        let candidate = format!("{base}-{suffix}");
        let candidate = if candidate.len() <= SCRIPT_ID_MAX {
            candidate
        } else {
            let keep = SCRIPT_ID_MAX
                .saturating_sub(suffix.to_string().len() + 1)
                .max(1);
            format!("{}-{suffix}", &base[..keep.min(base.len())])
        };
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    base
}

/// The list after a save: the edited action replaces its entry (or is appended), and the other
/// actions give up a setup or settle role the saved one claims (`releaseClaimedRoles`).
pub fn saved_scripts(current: &[Script], saved: Script) -> Vec<Script> {
    let mut updated: Vec<Script> = current
        .iter()
        .map(|script| {
            if script.id == saved.id {
                return saved.clone();
            }
            let mut other = script.clone();
            if saved.run_on_worktree_create {
                other.run_on_worktree_create = false;
            }
            if saved.run_on_settle {
                other.run_on_settle = false;
            }
            other
        })
        .collect();
    if !current.iter().any(|script| script.id == saved.id) {
        updated.push(saved);
    }
    updated
}

/// `derivePhysicalProjectKey`: the key a checkout's grouping rule is saved under.
pub fn grouping_key(environment_id: &str, root: &str) -> String {
    let trimmed = root.trim();
    let drive = trimmed.len() >= 2
        && trimmed.as_bytes()[0].is_ascii_alphabetic()
        && trimmed.as_bytes()[1] == b':';
    let is_root = matches!(trimmed, "/" | "\\")
        || (drive && trimmed.len() == 3 && matches!(trimmed.as_bytes()[2], b'/' | b'\\'));
    let path = if is_root {
        trimmed.to_owned()
    } else if trimmed.starts_with('/') {
        let kept = trimmed.trim_end_matches('/');
        if kept.is_empty() { trimmed } else { kept }.to_owned()
    } else {
        let kept = trimmed.trim_end_matches(['/', '\\']);
        if kept.is_empty() {
            trimmed.to_owned()
        } else if kept.len() == 2 && drive {
            format!("{kept}\\")
        } else {
            kept.to_owned()
        }
    };
    let windows = drive || path.starts_with("\\\\");
    let path = if windows {
        path.replace('/', "\\").to_lowercase()
    } else {
        path
    };
    format!("{environment_id}:{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(title: &str) -> Member {
        Member {
            id: format!("id-{title}"),
            title: title.to_owned(),
            root: format!("/work/{title}"),
            ..Member::default()
        }
    }

    #[test]
    fn an_update_names_the_project_and_only_the_changed_fields() {
        let payload = update_payload("p1", "c1".to_owned(), json!({"pinned": true}));
        assert_eq!(
            payload,
            json!({"type": "project.update", "commandId": "c1", "projectId": "p1", "pinned": true})
        );
        assert_eq!(
            delete_payload("p1", "c2".to_owned()),
            json!({"type": "project.delete", "commandId": "c2", "projectId": "p1", "force": true})
        );
    }

    #[test]
    fn a_rename_writes_only_when_a_member_differs() {
        let members = [member("alpha")];
        assert_eq!(title_to_write(&members, " alpha "), Ok(None));
        assert_eq!(
            title_to_write(&members, " beta "),
            Ok(Some("beta".to_owned()))
        );
        assert_eq!(
            title_to_write(&members, "  "),
            Err("Project title cannot be empty")
        );
    }

    #[test]
    fn the_danger_copy_follows_the_selection() {
        assert_eq!(danger(true, 1).button, "Remove checkout");
        let all = danger(false, 3);
        assert_eq!(all.button, "Remove all entries");
        assert!(all.description.contains("all 3 checkout entries"));
        assert_eq!(danger(false, 1).title, "Remove project");
    }

    #[test]
    fn the_confirmation_matches_plan_project_removal() {
        let one = [member("alpha")];
        assert_eq!(
            confirmation(&one, 1, false, "alpha", Some("Devbox"), 2),
            [
                "Remove project \"alpha\" and delete its 2 threads?",
                "Path: /work/alpha",
                "Environment: Devbox",
                "Those threads and any archived threads are deleted from T3 Code. Provider sessions are kept.",
                "This removes only the project entries, not the files on disk.",
                "This action cannot be undone.",
            ]
        );
        let two = [member("a"), member("b")];
        let lines = confirmation(&two, 3, true, "group", None, 0);
        assert_eq!(lines[0], "Remove checkout \"group\"?");
        assert_eq!(lines[1], "This removes 2 grouped project entries.");
        assert_eq!(
            lines[3],
            "Other entries in this grouped project are unaffected."
        );
    }

    #[test]
    fn the_icon_row_describes_the_override_then_the_favicon() {
        let mut project = member("alpha");
        assert_eq!(icon_description(&project), "Automatic");
        assert!(has_default_icon(&project));
        project.favicon_path = Some("icon.svg".to_owned());
        assert_eq!(icon_description(&project), "icon.svg");
        project.icon = Some("rocket \u{b7} sky".to_owned());
        assert_eq!(icon_description(&project), "rocket \u{b7} sky");
        assert!(!has_default_icon(&project));
    }

    #[test]
    fn an_action_round_trips_in_the_servers_shape() {
        let wire = json!({
            "id": "test", "name": "Test", "command": "bun test", "icon": "test",
            "runOnWorktreeCreate": true, "async": false, "runOnSettle": true,
            "previewUrl": "http://localhost:5173", "autoOpenPreview": true
        });
        let script = Script::decode(&wire).unwrap();
        assert!(script.wait_for_setup && script.run_on_settle && script.auto_open_preview);
        assert_eq!(script.encode(), wire);
        let mut plain = script;
        plain.run_on_worktree_create = false;
        plain.run_on_settle = false;
        plain.preview_url.clear();
        let encoded = plain.encode();
        assert!(encoded.get("async").is_none() && encoded.get("runOnSettle").is_none());
        assert!(encoded.get("previewUrl").is_none() && encoded.get("autoOpenPreview").is_none());
        assert_eq!(Script::decode(&json!({"id": "x", "name": "X"})), None);
    }

    #[test]
    fn action_ids_follow_next_project_script_id() {
        let taken = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        assert_eq!(next_script_id("Run Tests!", &[]), "run-tests");
        assert_eq!(next_script_id("  ", &[]), "script");
        assert_eq!(next_script_id("Test", &taken(&["test"])), "test-2");
        assert_eq!(
            next_script_id("Test", &taken(&["test", "test-2"])),
            "test-3"
        );
        let long = next_script_id("a very long action name indeed", &[]);
        assert!(long.len() <= 24 && !long.ends_with('-'));
        let again = next_script_id("a very long action name indeed", &taken(&[long.as_str()]));
        assert!(again.len() <= 24 && again != long);
    }

    #[test]
    fn saving_an_action_releases_the_roles_it_claims() {
        let mut setup = Script::blank();
        setup.id = "setup".to_owned();
        setup.run_on_worktree_create = true;
        setup.run_on_settle = true;
        let mut other = Script::blank();
        other.id = "other".to_owned();
        let mut claims = Script::blank();
        claims.id = "claims".to_owned();
        claims.run_on_worktree_create = true;
        let list = saved_scripts(&[setup.clone(), other.clone()], claims.clone());
        assert_eq!(list.len(), 3);
        assert!(!list[0].run_on_worktree_create && list[0].run_on_settle);
        assert_eq!(list[2], claims);
        let edited = Script {
            name: "Renamed".to_owned(),
            ..other
        };
        assert_eq!(saved_scripts(&list, edited.clone())[1], edited);
        assert_eq!(saved_scripts(&list, edited).len(), 3);
        assert_eq!(script_problem(" ", "x"), Some("Name is required."));
        assert_eq!(script_problem("a", " "), Some("Command is required."));
        assert_eq!(script_problem("a", "b"), None);
    }

    #[test]
    fn a_checkouts_grouping_key_is_the_environment_and_its_normalized_path() {
        assert_eq!(grouping_key("env1", " /work/app/ "), "env1:/work/app");
        assert_eq!(grouping_key("env1", "/"), "env1:/");
        assert_eq!(grouping_key("env1", "C:/Work/App/"), "env1:c:\\work\\app");
    }
}
