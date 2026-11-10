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
}
