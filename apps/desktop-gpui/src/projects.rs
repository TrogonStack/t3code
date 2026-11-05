//! The project picker's rows, mirroring the web picker: `sortLogicalProjectsForSidebar`
//! (Sidebar.logic.ts), `pickableProjectGroups` (projectGrouping.ts), `deriveProjectIdentity`
//! (projectIdentity.ts) and `buildSidebarProjectPickerEntries` (sidebarProjectGrouping.ts).

use crate::wire::{Project, Thread};
use std::collections::HashMap;

/// `[name, light, dark]` as sRGB hex: Tailwind's 600 and 400 shades, in `PROJECT_ICON_COLORS` order.
const COLORS: [(&str, u32, u32); 18] = [
    ("gray", 0x4b5563, 0x9ca3af),
    ("red", 0xdc2626, 0xf87171),
    ("orange", 0xea580c, 0xfb923c),
    ("amber", 0xd97706, 0xfbbf24),
    ("yellow", 0xca8a04, 0xfacc15),
    ("lime", 0x65a30d, 0xa3e635),
    ("green", 0x16a34a, 0x4ade80),
    ("emerald", 0x059669, 0x34d399),
    ("teal", 0x0d9488, 0x2dd4bf),
    ("cyan", 0x0891b2, 0x22d3ee),
    ("sky", 0x0284c7, 0x38bdf8),
    ("blue", 0x2563eb, 0x60a5fa),
    ("indigo", 0x4f46e5, 0x818cf8),
    ("violet", 0x7c3aed, 0xa78bfa),
    ("purple", 0x9333ea, 0xc084fc),
    ("fuchsia", 0xc026d3, 0xe879f9),
    ("pink", 0xdb2777, 0xf472b6),
    ("rose", 0xe11d48, 0xfb7185),
];

/// The server's title for the managed no-project workspace (ManagedProjectFolders.ts).
const SCRATCH_TITLE: &str = "No project";
const SCRATCH_FOLDER: &str = "scratch";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectEntry {
    pub id: String,
    pub title: String,
    pub initials: String,
    /// Index into the shared palette.
    pub color: usize,
    pub pinned: bool,
    pub scratch: bool,
}

impl ProjectEntry {
    /// The badge text color for the theme; the badge background is this at 14% alpha.
    pub fn color_hex(&self, dark: bool) -> u32 {
        let (_, light, night) = COLORS[self.color];
        if dark { night } else { light }
    }
}

fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

fn initials(name: &str) -> String {
    let words = words(name);
    let Some(first_word) = words.first() else {
        return "PR".to_owned();
    };
    let glyphs: Vec<char> = first_word.chars().collect();
    let first = glyphs[0];
    let second = glyphs[1..]
        .iter()
        .copied()
        .find(|glyph| glyph.is_numeric())
        .or_else(|| {
            if words.len() > 1 {
                words.last().and_then(|word| word.chars().next())
            } else {
                glyphs.last().copied()
            }
        })
        .unwrap_or(first);
    format!("{first}{second}")
        .to_uppercase()
        .chars()
        .take(2)
        .collect()
}

fn color_index(name: &str) -> usize {
    let lowered = name.trim().to_lowercase();
    let seed = if lowered.is_empty() {
        "project"
    } else {
        &lowered
    };
    seed.chars().fold(0, |index, glyph| {
        (index * 31 + glyph as usize) % COLORS.len()
    })
}

/// A monogram icon override wins, as in `ProjectFavicon`; other icon kinds fall back to the derived one.
fn monogram_override(project: &Project) -> Option<(String, usize)> {
    let icon = project.project_icon.as_ref()?;
    if icon.get("kind")?.as_str()? != "monogram" {
        return None;
    }
    let text = icon.get("text")?.as_str()?.to_owned();
    let color = icon.get("color")?.as_str()?;
    let index = COLORS.iter().position(|(name, ..)| *name == color)?;
    Some((text, index))
}

/// The server creates the scratch project at `<base dir>/scratch` titled "No project", and the
/// shell does not carry the server config's `scratchWorkspaceRoot`, so match on both.
fn is_scratch(project: &Project) -> bool {
    let root = project.workspace_root.trim_end_matches(['/', '\\']);
    project.title == SCRATCH_TITLE
        && root
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|folder| folder == SCRATCH_FOLDER)
}

/// ISO timestamps from the server are fixed-width UTC, so text order is time order.
fn thread_activity(thread: &Thread) -> &str {
    thread
        .latest_user_message_at
        .as_deref()
        .unwrap_or(&thread.updated_at)
}

/// Unarchived projects, pinned first, then most recent thread activity (the project's own
/// `updatedAt` when it has no threads), then title.
pub fn build_entries<'a>(
    projects: impl IntoIterator<Item = &'a Project>,
    threads: impl IntoIterator<Item = &'a Thread>,
) -> Vec<ProjectEntry> {
    let mut latest: HashMap<&str, &str> = HashMap::new();
    for thread in threads.into_iter().filter(|t| t.archived_at.is_none()) {
        let activity = thread_activity(thread);
        let slot = latest.entry(thread.project_id.as_str()).or_insert(activity);
        *slot = (*slot).max(activity);
    }
    let mut ranked: Vec<(&Project, &str)> = projects
        .into_iter()
        .filter(|project| project.archived_at.is_none())
        .map(|project| {
            let activity = latest
                .get(project.id.as_str())
                .copied()
                .unwrap_or(&project.updated_at);
            (project, activity)
        })
        .collect();
    ranked.sort_by(|(a, a_at), (b, b_at)| {
        b.pinned_at
            .is_some()
            .cmp(&a.pinned_at.is_some())
            .then_with(|| b_at.cmp(a_at))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    ranked
        .into_iter()
        .map(|(project, _)| {
            let (initials, color) = monogram_override(project)
                .unwrap_or_else(|| (initials(&project.title), color_index(&project.title)));
            ProjectEntry {
                id: project.id.clone(),
                title: project.title.clone(),
                initials,
                color,
                pinned: project.pinned_at.is_some(),
                scratch: is_scratch(project),
            }
        })
        .collect()
}

/// What the picker lists: no scratch project, the active project leading the unpinned ones
/// (pinned stay first), then the case-insensitive title filter.
pub fn picker_entries<'a>(
    entries: &'a [ProjectEntry],
    active_id: Option<&str>,
    query: &str,
) -> Vec<&'a ProjectEntry> {
    let mut listed: Vec<&ProjectEntry> = entries.iter().filter(|entry| !entry.scratch).collect();
    if let Some(at) = listed
        .iter()
        .position(|entry| Some(entry.id.as_str()) == active_id && !entry.pinned)
    {
        let active = listed.remove(at);
        let pinned = listed.iter().filter(|entry| entry.pinned).count();
        listed.insert(pinned, active);
    }
    let query = query.trim().to_lowercase();
    listed.retain(|entry| entry.title.to_lowercase().contains(&query));
    listed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SNAPSHOT: &str = include_str!("../tests/fixtures/shell-snapshot.json");

    fn project(id: &str, title: &str, edit: impl FnOnce(&mut Value)) -> Project {
        let mut value: Value = serde_json::from_str(SNAPSHOT).expect("json");
        let mut project = value["snapshot"]["projects"][0].take();
        project["id"] = id.into();
        project["title"] = title.into();
        project["archivedAt"] = Value::Null;
        project["pinnedAt"] = Value::Null;
        project["updatedAt"] = "2026-01-01T00:00:00.000Z".into();
        edit(&mut project);
        serde_json::from_value(project).expect("project decodes")
    }

    fn thread(project_id: &str, user_message_at: Option<&str>, updated_at: &str) -> Thread {
        let mut value: Value = serde_json::from_str(SNAPSHOT).expect("json");
        let mut thread = value["snapshot"]["threads"][0].take();
        thread["id"] = format!("{project_id}-{updated_at}").into();
        thread["projectId"] = project_id.into();
        thread["latestUserMessageAt"] = user_message_at.into();
        thread["updatedAt"] = updated_at.into();
        serde_json::from_value(thread).expect("thread decodes")
    }

    fn titles(entries: &[&ProjectEntry]) -> Vec<String> {
        entries.iter().map(|entry| entry.title.clone()).collect()
    }

    #[test]
    fn initials_follow_the_web_monogram() {
        for (name, expected) in [
            ("atlas", "AS"),
            ("fx-workspace", "FW"),
            ("agent0", "A0"),
            ("t3code", "T3"),
            ("x", "XX"),
            ("  ", "PR"),
            ("--", "PR"),
        ] {
            assert_eq!(initials(name), expected, "{name}");
        }
    }

    #[test]
    fn color_is_deterministic_and_case_insensitive() {
        assert_eq!(color_index("Atlas"), color_index(" atlas "));
        assert_eq!(color_index(""), color_index("project"));
        assert_eq!(COLORS[color_index("a")].0, COLORS[97 % 18].0);
        assert!(color_index("anything at all") < COLORS.len());
    }

    #[test]
    fn monogram_icon_overrides_the_derived_badge() {
        let overridden = project("p", "atlas", |p| {
            p["projectIcon"] = json!({"kind": "monogram", "text": "ZZ", "color": "rose"});
        });
        let lucide = project("q", "atlas", |p| {
            p["projectIcon"] = json!({"kind": "lucide", "name": "star", "color": "rose"});
        });
        let entries = build_entries([&overridden, &lucide], []);
        let by_id = |id: &str| entries.iter().find(|e| e.id == id).unwrap();
        assert_eq!(by_id("p").initials, "ZZ");
        assert_eq!(COLORS[by_id("p").color].0, "rose");
        assert_eq!(by_id("q").initials, "AS");
    }

    #[test]
    fn entries_order_pinned_first_then_recent_activity_then_title() {
        let projects = [
            project("quiet", "quiet", |p| {
                p["updatedAt"] = "2026-03-01T00:00:00.000Z".into()
            }),
            project("busy", "busy", |_| {}),
            project("pinned", "pinned", |p| {
                p["pinnedAt"] = "2026-01-01T00:00:00.000Z".into()
            }),
            project("tie-b", "Bravo", |_| {}),
            project("tie-a", "alpha", |_| {}),
            project("gone", "gone", |p| {
                p["archivedAt"] = "2026-01-01T00:00:00.000Z".into()
            }),
        ];
        let mut archived = thread("busy", None, "2026-09-01T00:00:00.000Z");
        archived.archived_at = Some("2026-09-02T00:00:00.000Z".into());
        let threads = [
            // The user message time wins over a newer updatedAt.
            thread(
                "busy",
                Some("2026-05-01T00:00:00.000Z"),
                "2026-06-01T00:00:00.000Z",
            ),
            thread("busy", None, "2026-04-01T00:00:00.000Z"),
            archived,
        ];
        let entries = build_entries(&projects, &threads);
        let order: Vec<_> = entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(order, ["pinned", "busy", "quiet", "alpha", "Bravo"]);
        assert!(entries[0].pinned && !entries[1].pinned);
    }

    #[test]
    fn scratch_is_the_managed_no_project_folder() {
        let scratch = project("s", "No project", |p| {
            p["workspaceRoot"] = "/home/me/.t3/userdata/scratch/".into()
        });
        let titled_only = project("t", "No project", |p| {
            p["workspaceRoot"] = "/work/app".into()
        });
        let folder_only = project("f", "scratch", |p| {
            p["workspaceRoot"] = "/work/scratch".into()
        });
        let entries = build_entries([&scratch, &titled_only, &folder_only], []);
        let flags: Vec<_> = ["s", "t", "f"]
            .iter()
            .map(|id| entries.iter().find(|e| e.id == *id).unwrap().scratch)
            .collect();
        assert_eq!(flags, [true, false, false]);
        assert_eq!(
            titles(&picker_entries(&entries, None, "")).len(),
            2,
            "scratch is left out"
        );
    }

    #[test]
    fn picker_leads_with_the_active_unpinned_project_and_filters() {
        let projects = [
            project("pin", "pinned", |p| {
                p["pinnedAt"] = "2026-01-01T00:00:00.000Z".into()
            }),
            project("a", "Alpha", |p| {
                p["updatedAt"] = "2026-03-01T00:00:00.000Z".into()
            }),
            project("b", "Beta", |p| {
                p["updatedAt"] = "2026-02-01T00:00:00.000Z".into()
            }),
            project("c", "Gamma", |_| {}),
        ];
        let entries = build_entries(&projects, []);
        assert_eq!(
            titles(&picker_entries(&entries, None, "")),
            ["pinned", "Alpha", "Beta", "Gamma"]
        );
        assert_eq!(
            titles(&picker_entries(&entries, Some("c"), "")),
            ["pinned", "Gamma", "Alpha", "Beta"]
        );
        assert_eq!(
            titles(&picker_entries(&entries, Some("pin"), "")),
            ["pinned", "Alpha", "Beta", "Gamma"]
        );
        assert_eq!(
            titles(&picker_entries(&entries, Some("c"), "  ETA ")),
            ["Beta"]
        );
        assert!(picker_entries(&entries, None, "zzz").is_empty());
    }
}
