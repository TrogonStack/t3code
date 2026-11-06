//! The project picker's rows, mirroring the web picker in its default `repository` grouping mode:
//! `buildProjectGroups` and `pickableProjectGroups` (projectGrouping.ts),
//! `sortLogicalProjectsForSidebar` (Sidebar.logic.ts) and `buildSidebarProjectPickerEntries`
//! (sidebarProjectGrouping.ts). Each row is a group of projects that share a repository.

use crate::wire::generated::RepositoryIdentity;
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

/// `EnvironmentMachineKind` (packages/contracts/src/environment.ts); unknown values read as a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineKind {
    Server,
    Cloud,
    Linux,
    Desktop,
    Laptop,
    MacMini,
    MacStudio,
}

impl MachineKind {
    pub fn parse(kind: &str) -> Self {
        match kind {
            "cloud" => Self::Cloud,
            "linux" => Self::Linux,
            "desktop" => Self::Desktop,
            "laptop" => Self::Laptop,
            "mac-mini" => Self::MacMini,
            "mac-studio" => Self::MacStudio,
            _ => Self::Server,
        }
    }
}

/// Another environment that also has this project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteMember {
    pub label: String,
    pub machine: MachineKind,
}

/// A saved environment other than the one this window attaches to, with its projects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentProjects {
    pub id: String,
    pub label: String,
    pub machine: MachineKind,
    pub projects: Vec<ProjectEntry>,
}

/// One picker row: a `ProjectGroup` of the web, seen from the attached environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectEntry {
    /// The group key: the repository's canonical key, else the project's physical path.
    pub key: String,
    /// The target project (the representative): what picking the row selects.
    pub id: String,
    /// Every project of the group on the attached environment, the pin writes all of them.
    pub member_ids: Vec<String>,
    /// The group label (`deriveProjectGroupLabel`).
    pub title: String,
    /// From the representative: its monogram override, else its title.
    pub initials: String,
    /// Index into the shared palette.
    pub color: usize,
    /// Any member is pinned.
    pub pinned: bool,
    pub scratch: bool,
    /// The representative's workspace root, where the composer strip reads the git branch.
    pub root: String,
    /// The repository key shared across environments, when the group has a repository.
    pub logical_key: Option<String>,
    /// Other environments that have the same logical project.
    pub remote: Vec<RemoteMember>,
}

impl ProjectEntry {
    pub fn contains(&self, project_id: &str) -> bool {
        self.member_ids.iter().any(|id| id == project_id)
    }

    /// The project picking the row selects: the active one when it belongs to the group, as the
    /// web's `targetProject`, else the representative.
    pub fn target<'a>(&'a self, active_id: Option<&'a str>) -> &'a str {
        active_id.filter(|id| self.contains(id)).unwrap_or(&self.id)
    }

    /// The badge text color for the theme (gray for the scratch project, whose server-set icon is gray); the badge background is this at 14% alpha.
    pub fn color_hex(&self, dark: bool) -> u32 {
        let (_, light, night) = COLORS[if self.scratch { 0 } else { self.color }];
        if dark { night } else { light }
    }
}

/// The pin button's tooltip, with the default `thread.pin` shortcut.
pub fn pin_tooltip(name: &str, pinned: bool) -> String {
    let verb = if pinned { "Unpin" } else { "Pin" };
    format!("{verb} {name} (\u{21e7}\u{2318}P)")
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

/// The cross-environment identity of `projectGrouping.ts` in the default `repository` mode: the
/// repository's canonical key. Without one the web falls back to a per-environment path key, which
/// never matches another environment, so there is nothing to join on.
fn logical_key(project: &Project) -> Option<String> {
    let identity = project.repository_identity.as_ref()?;
    let key = identity
        .origin
        .as_ref()
        .map_or(&identity.canonical_key, |origin| &origin.canonical_key);
    (!key.is_empty() && !key.contains('\0')).then(|| key.clone())
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

/// `normalizeProjectPathForComparison`: trimmed, no trailing separators, Windows paths folded.
fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    let stripped = trimmed.trim_end_matches(['/', '\\']);
    let normalized = if stripped.is_empty() && !trimmed.is_empty() {
        &trimmed[..1]
    } else {
        stripped
    };
    let bytes = normalized.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if drive || normalized.starts_with("\\\\") {
        normalized.replace('/', "\\").to_lowercase()
    } else {
        normalized.to_owned()
    }
}

/// `shouldReplacePhysicalProjectWinner`: the fresher project wins, then the greater id. Server
/// timestamps are fixed-width UTC, so text order is time order.
fn fresher(candidate: &Project, existing: &Project) -> bool {
    let key = |project: &Project| (project.updated_at.clone(), project.id.clone());
    key(candidate) > key(existing)
}

fn unique_non_empty<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Vec<&'a str> {
    let mut unique: Vec<&str> = Vec::new();
    for value in values.into_iter().flatten().map(str::trim) {
        if !value.is_empty() && !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique
}

/// `repositoryGroupingDisplayNameOf`.
fn grouping_display_name(identity: &RepositoryIdentity) -> Option<&str> {
    match &identity.origin {
        Some(origin) => Some(
            origin
                .display_name
                .as_deref()
                .unwrap_or(&origin.canonical_key),
        ),
        None => identity.display_name.as_deref(),
    }
}

/// `deriveProjectGroupLabel`, for groups of more than one member.
fn group_label(representative: &Project, members: &[&Project]) -> String {
    let titles = unique_non_empty(members.iter().map(|m| Some(m.title.as_str())));
    let display_names = unique_non_empty(members.iter().map(|m| {
        m.repository_identity
            .as_ref()
            .and_then(grouping_display_name)
    }));
    let names = unique_non_empty(members.iter().map(|m| {
        m.repository_identity
            .as_ref()
            .and_then(|i| i.name.as_deref())
    }));
    if let [title] = titles[..]
        && !display_names.contains(&title)
        && !names.contains(&title)
    {
        return title.to_owned();
    }
    if let [name] = display_names[..] {
        return name.to_owned();
    }
    if let [name] = names[..] {
        return name.to_owned();
    }
    representative.title.clone()
}

/// Group keys without a repository; the NUL keeps them out of the repository key space.
const PATH_KEY_PREFIX: &str = "\0path:";

/// A group under construction: the physical winners, plus every project they stand for.
struct Group<'a> {
    key: String,
    winners: Vec<&'a Project>,
    all: Vec<&'a Project>,
}

/// `buildProjectGroups` in `repository` mode: projects with the same normalized workspace root
/// collapse to the freshest, then groups form on the repository key (the project's own path when
/// it has none). The web's `projects` input lists a server's projects in snapshot order, which is
/// creation order, so members are ordered by `createdAt` here (the shell state is a map) and the
/// first one is the representative. Every project here is on the attached environment.
fn build_groups<'a>(projects: impl IntoIterator<Item = &'a Project>) -> Vec<Group<'a>> {
    let mut ordered: Vec<&Project> = projects.into_iter().collect();
    ordered.sort_by(|a, b| (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id)));

    let mut physical: Vec<(String, Vec<&Project>)> = Vec::new();
    for project in ordered {
        let key = normalize_path(&project.workspace_root);
        match physical.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, members)) => members.push(project),
            None => physical.push((key, vec![project])),
        }
    }

    let mut groups: Vec<Group> = Vec::new();
    for (path, projects) in physical {
        let winner = projects
            .iter()
            .copied()
            .reduce(|current, candidate| {
                if fresher(candidate, current) {
                    candidate
                } else {
                    current
                }
            })
            .expect("a physical key has a project");
        // `selectProjectIdentitySource`: a winner without a repository borrows the freshest identity.
        let source = if winner.repository_identity.is_some() {
            winner
        } else {
            projects
                .iter()
                .copied()
                .filter(|project| project.repository_identity.is_some())
                .reduce(|current, candidate| {
                    if fresher(candidate, current) {
                        candidate
                    } else {
                        current
                    }
                })
                .unwrap_or(winner)
        };
        let key = logical_key(source).unwrap_or_else(|| format!("{PATH_KEY_PREFIX}{path}"));
        match groups.iter_mut().find(|group| group.key == key) {
            Some(group) => {
                group.winners.push(winner);
                group.all.extend(projects);
            }
            None => groups.push(Group {
                key,
                winners: vec![winner],
                all: projects,
            }),
        }
    }
    groups
}

/// The picker's groups: pinned first, then most recent thread activity across the group (the
/// representative's `updatedAt` when it has no threads), then label. Groups whose members are all
/// archived are left out.
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
    let mut ranked: Vec<(ProjectEntry, String)> = build_groups(projects)
        .into_iter()
        .filter(|group| group.winners.iter().any(|p| p.archived_at.is_none()))
        .map(|group| {
            let representative = group.winners[0];
            let label = if group.winners.len() > 1 {
                group_label(representative, &group.winners)
            } else {
                representative.title.clone()
            };
            let (initials, color) = monogram_override(representative).unwrap_or_else(|| {
                (
                    initials(&representative.title),
                    color_index(&representative.title),
                )
            });
            let activity = group
                .all
                .iter()
                .filter_map(|p| latest.get(p.id.as_str()).copied())
                .max()
                .unwrap_or(&representative.updated_at)
                .to_owned();
            let entry = ProjectEntry {
                logical_key: (!group.key.starts_with(PATH_KEY_PREFIX)).then(|| group.key.clone()),
                key: group.key,
                id: representative.id.clone(),
                member_ids: group.all.iter().map(|p| p.id.clone()).collect(),
                title: label,
                initials,
                color,
                pinned: group.winners.iter().any(|p| p.pinned_at.is_some()),
                scratch: is_scratch(representative),
                root: representative.workspace_root.clone(),
                remote: Vec::new(),
            };
            (entry, activity)
        })
        .collect();
    ranked.sort_by(|(a, a_at), (b, b_at)| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b_at.cmp(a_at))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then_with(|| a.key.cmp(&b.key))
    });
    ranked.into_iter().map(|(entry, _)| entry).collect()
}

/// Adds to each entry the other environments that have the same logical project.
pub fn with_remote_members(
    entries: &[ProjectEntry],
    others: &[EnvironmentProjects],
) -> Vec<ProjectEntry> {
    entries
        .iter()
        .map(|entry| {
            let mut entry = entry.clone();
            if let Some(key) = &entry.logical_key {
                entry.remote = others
                    .iter()
                    .filter(|other| {
                        other
                            .projects
                            .iter()
                            .any(|project| project.logical_key.as_ref() == Some(key))
                    })
                    .map(|other| RemoteMember {
                        label: if other.label.trim().is_empty() {
                            "Remote".to_owned()
                        } else {
                            other.label.clone()
                        },
                        machine: other.machine,
                    })
                    .collect();
            }
            entry
        })
        .collect()
}

/// Members ordered by label, as `ProjectEnvironmentBadge` sorts them.
fn sorted_members(members: &[RemoteMember]) -> Vec<&RemoteMember> {
    let mut sorted: Vec<&RemoteMember> = members.iter().collect();
    sorted.sort_by_cached_key(|member| (member.label.to_lowercase(), member.label.clone()));
    sorted
}

/// The environment badge's tooltip. Picker rows are always on the attached environment, so this is
/// the web's "Also on" case; "On" only applies to groups the attached environment lacks.
pub fn badge_text(members: &[RemoteMember]) -> Option<String> {
    let mut labels: Vec<&str> = Vec::new();
    for member in sorted_members(members) {
        if !labels.contains(&member.label.as_str()) {
            labels.push(&member.label);
        }
    }
    (!labels.is_empty()).then(|| format!("Also on {}", labels.join(", ")))
}

/// The badge icon follows the first member by label.
pub fn badge_machine(members: &[RemoteMember]) -> Option<MachineKind> {
    sorted_members(members).first().map(|member| member.machine)
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
        .position(|entry| active_id.is_some_and(|id| entry.contains(id)) && !entry.pinned)
    {
        let active = listed.remove(at);
        let pinned = listed.iter().filter(|entry| entry.pinned).count();
        listed.insert(pinned, active);
    }
    let query = query.trim().to_lowercase();
    listed.retain(|entry| entry.title.to_lowercase().contains(&query));
    listed
}

/// Rows in the order they had when the picker opened (`useOrderWhileOpen`): keys that left drop
/// out, keys that arrived follow in their current order.
pub fn order_while_open<'a>(
    listed: Vec<&'a ProjectEntry>,
    opened_with: &[String],
) -> Vec<&'a ProjectEntry> {
    let position = |entry: &ProjectEntry| {
        opened_with
            .iter()
            .position(|key| *key == entry.key)
            .unwrap_or(opened_with.len())
    };
    let mut ordered = listed;
    ordered.sort_by_key(|entry| position(entry));
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SNAPSHOT: &str = include_str!("../tests/fixtures/shell-snapshot.json");

    #[test]
    fn order_while_open_keeps_the_opening_order_and_appends_new_rows() {
        let entry = |id: &str, pinned: bool| ProjectEntry {
            key: id.to_owned(),
            id: id.to_owned(),
            member_ids: vec![id.to_owned()],
            title: id.to_owned(),
            initials: String::new(),
            color: 0,
            pinned,
            scratch: false,
            root: String::new(),
            logical_key: None,
            remote: Vec::new(),
        };
        let opened = ["a", "b", "c"].map(str::to_owned);
        let now = [entry("d", false), entry("c", true), entry("a", false)];
        let ids = |listed: Vec<&ProjectEntry>| -> Vec<String> {
            listed.into_iter().map(|e| e.id.clone()).collect()
        };
        assert_eq!(
            ids(order_while_open(now.iter().collect(), &opened)),
            ["a", "c", "d"]
        );
        assert_eq!(
            ids(order_while_open(now.iter().collect(), &[])),
            ["d", "c", "a"]
        );
    }

    #[test]
    fn pin_tooltip_names_the_action_and_shortcut() {
        assert_eq!(
            pin_tooltip("t3code", false),
            "Pin t3code (\u{21e7}\u{2318}P)"
        );
        assert_eq!(
            pin_tooltip("t3code", true),
            "Unpin t3code (\u{21e7}\u{2318}P)"
        );
    }

    fn project(id: &str, title: &str, edit: impl FnOnce(&mut Value)) -> Project {
        let mut value: Value = serde_json::from_str(SNAPSHOT).expect("json");
        let mut project = value["snapshot"]["projects"][0].take();
        project["id"] = id.into();
        project["title"] = title.into();
        project["archivedAt"] = Value::Null;
        project["pinnedAt"] = Value::Null;
        project["updatedAt"] = "2026-01-01T00:00:00.000Z".into();
        project["createdAt"] = "2026-01-01T00:00:00.000Z".into();
        project["workspaceRoot"] = format!("/work/{id}").into();
        project["repositoryIdentity"] = Value::Null;
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

    fn repo(key: &str) -> serde_json::Value {
        json!({
            "canonicalKey": key,
            "locator": {"remoteName": "origin", "remoteUrl": "git@example.invalid:o/r.git", "source": "git-remote"},
        })
    }

    #[test]
    fn logical_key_prefers_the_origin_key_and_needs_a_repository() {
        let plain = project("a", "a", |p| p["repositoryIdentity"] = repo("host/o/r"));
        let forked = project("b", "b", |p| {
            let mut identity = repo("host/fork/r");
            identity["origin"] = json!({"canonicalKey": "host/o/r"});
            p["repositoryIdentity"] = identity;
        });
        let bare = project("c", "c", |p| p["repositoryIdentity"] = Value::Null);
        let keys: Vec<_> = [&plain, &forked, &bare]
            .iter()
            .map(|p| logical_key(p))
            .collect();
        assert_eq!(
            keys,
            [
                Some("host/o/r".to_owned()),
                Some("host/o/r".to_owned()),
                None
            ]
        );
    }

    #[test]
    fn remote_members_join_on_the_logical_key() {
        let entries = build_entries(
            &[
                project("a", "atlas", |p| {
                    p["repositoryIdentity"] = repo("host/o/atlas")
                }),
                project("b", "solo", |p| p["repositoryIdentity"] = Value::Null),
            ],
            [],
        );
        let other = |label: &str, machine, key: &str| EnvironmentProjects {
            id: label.to_owned(),
            label: label.to_owned(),
            machine,
            projects: build_entries(
                &[project("r", "atlas", |p| {
                    p["repositoryIdentity"] = repo(key)
                })],
                [],
            ),
        };
        let others = [
            other("devbox1", MachineKind::Linux, "host/o/atlas"),
            other("elsewhere", MachineKind::Cloud, "host/o/other"),
        ];
        let joined = with_remote_members(&entries, &others);
        let by_title = |t: &str| joined.iter().find(|e| e.title == t).unwrap();
        assert_eq!(
            by_title("atlas").remote,
            [RemoteMember {
                label: "devbox1".to_owned(),
                machine: MachineKind::Linux
            }]
        );
        assert!(by_title("solo").remote.is_empty());

        // A group on the other environment joins the same way, through its repository key.
        let grouped = EnvironmentProjects {
            projects: build_entries(
                &[
                    project("r1", "atlas", |p| {
                        p["repositoryIdentity"] = repo("host/o/atlas")
                    }),
                    project("r2", "api", |p| {
                        p["repositoryIdentity"] = repo("host/o/atlas")
                    }),
                ],
                [],
            ),
            ..other("devbox2", MachineKind::Server, "x")
        };
        let joined = with_remote_members(&entries, &[grouped]);
        assert_eq!(by_title("atlas").remote.len(), 1);
        assert_eq!(
            joined.iter().find(|e| e.title == "atlas").unwrap().remote[0].label,
            "devbox2"
        );
    }

    #[test]
    fn badge_lists_unique_labels_and_takes_the_first_machine() {
        let member = |label: &str, machine| RemoteMember {
            label: label.to_owned(),
            machine,
        };
        assert_eq!(badge_text(&[]), None);
        assert_eq!(badge_machine(&[]), None);
        let members = [
            member("devbox1", MachineKind::Linux),
            member("Air", MachineKind::Laptop),
            member("devbox1", MachineKind::Linux),
        ];
        assert_eq!(
            badge_text(&members).as_deref(),
            Some("Also on Air, devbox1")
        );
        assert_eq!(badge_machine(&members), Some(MachineKind::Laptop));
        assert_eq!(MachineKind::parse("mac-studio"), MachineKind::MacStudio);
        assert_eq!(MachineKind::parse("quantum"), MachineKind::Server);
    }

    fn named_repo(key: &str, display: &str, name: &str) -> Value {
        let mut identity = repo(key);
        identity["displayName"] = display.into();
        identity["name"] = name.into();
        identity
    }

    #[test]
    fn same_repository_projects_form_one_group_like_the_web() {
        let identity = || named_repo("host/trogon/t3code", "TrogonStack/t3code", "t3code");
        let projects = [
            project("root", "t3code", |p| {
                p["workspaceRoot"] = "/work/t3code/".into();
                p["repositoryIdentity"] = identity();
                p["createdAt"] = "2026-01-01T00:00:00.000Z".into();
                p["projectIcon"] = json!({"kind": "monogram", "text": "ZZ", "color": "rose"});
            }),
            project("server", "server", |p| {
                p["workspaceRoot"] = "/work/t3code/apps/server".into();
                p["repositoryIdentity"] = identity();
                p["createdAt"] = "2026-02-01T00:00:00.000Z".into();
                p["pinnedAt"] = "2026-03-01T00:00:00.000Z".into();
            }),
            // Same physical path as "root": the fresher one wins, the loser still counts as a member.
            project("root-dup", "t3code", |p| {
                p["workspaceRoot"] = "/work/t3code".into();
                p["updatedAt"] = "2026-01-02T00:00:00.000Z".into();
                p["createdAt"] = "2026-03-01T00:00:00.000Z".into();
            }),
            project("lone", "lone", |_| {}),
        ];
        let entries = build_entries(&projects, []);
        assert_eq!(
            titles(&entries.iter().collect::<Vec<_>>()),
            ["TrogonStack/t3code", "lone"]
        );
        let group = &entries[0];
        assert_eq!(group.key, "host/trogon/t3code");
        assert_eq!(group.logical_key.as_deref(), Some("host/trogon/t3code"));
        // The duplicate path takes its identity from the freshest identified project, and the
        // oldest winner (root-dup replaced root as winner) leads.
        assert_eq!(group.member_ids, ["root", "root-dup", "server"]);
        assert_eq!(group.id, "root-dup");
        assert!(group.pinned, "any member pinned");
        assert!(group.contains("server") && !group.contains("lone"));
        assert_eq!(group.target(Some("server")), "server");
        assert_eq!(group.target(Some("lone")), "root-dup");
        assert_eq!(entries[1].key, "\0path:/work/lone");
        assert_eq!(entries[1].logical_key, None);
        // The icon comes from the representative: root-dup has none, so its title is used.
        assert_eq!(group.initials, "T3");

        let mixed = [
            project("a", "a", |p| {
                p["repositoryIdentity"] = repo("k");
                p["archivedAt"] = "2026-01-01T00:00:00.000Z".into();
            }),
            project("b", "b", |p| p["repositoryIdentity"] = repo("k")),
        ];
        assert_eq!(
            build_entries(&mixed, []).len(),
            1,
            "archived only when every member is"
        );
        let all_archived = [
            project("a", "a", |p| {
                p["repositoryIdentity"] = repo("k");
                p["archivedAt"] = "2026-01-01T00:00:00.000Z".into();
            }),
            project("b", "b", |p| {
                p["repositoryIdentity"] = repo("k");
                p["archivedAt"] = "2026-01-01T00:00:00.000Z".into();
            }),
        ];
        assert!(build_entries(&all_archived, []).is_empty());
    }

    #[test]
    fn group_label_follows_derive_project_group_label() {
        let label = |rows: &[(&str, Value)]| {
            let projects: Vec<Project> = rows
                .iter()
                .enumerate()
                .map(|(index, (title, identity))| {
                    project(&format!("p{index}"), title, |p| {
                        p["repositoryIdentity"] = identity.clone();
                        p["createdAt"] = format!("2026-01-0{}T00:00:00.000Z", index + 1).into();
                    })
                })
                .collect();
            build_entries(&projects, [])[0].title.clone()
        };
        let named = || named_repo("k", "Org/app", "app");
        let nameless = || {
            let mut identity = repo("k");
            identity["name"] = "app".into();
            identity
        };
        // A shared title wins unless it is the display or repository name.
        assert_eq!(label(&[("mono", named()), ("mono", named())]), "mono");
        assert_eq!(
            label(&[("Org/app", named()), ("Org/app", named())]),
            "Org/app"
        );
        assert_eq!(label(&[("app", named()), ("app", named())]), "Org/app");
        // Different titles: the single display name, else the single repository name.
        assert_eq!(label(&[("app", named()), ("server", named())]), "Org/app");
        assert_eq!(label(&[("web", nameless()), ("server", nameless())]), "app");
        // Nothing shared: the representative's title.
        assert_eq!(label(&[("web", repo("k")), ("server", repo("k"))]), "web");
        // A lone project keeps its own title.
        assert_eq!(label(&[("app", named())]), "app");
    }
}
