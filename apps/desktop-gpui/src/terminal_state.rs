//! A thread's terminals and how they are grouped, ported from `terminalUiStateStore.ts`
//! (`upsertTerminalIntoGroups`, `closeThreadTerminal`), `packages/shared/src/terminalLabels.ts`
//! and `MAX_TERMINALS_PER_GROUP`.

/// `MAX_TERMINALS_PER_GROUP`.
pub const MAX_PER_GROUP: usize = 4;

/// A set of terminals shown together: side by side, or stacked when `vertical`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub ids: Vec<String>,
    pub vertical: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// A new terminal in its own group.
    New,
    /// A new terminal beside (or, when `vertical`, below) the active one.
    Split { vertical: bool },
}

/// The terminals of one thread, in the order they were made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Terminals {
    pub groups: Vec<Group>,
    pub active: String,
    pub active_group: String,
}

fn group_id(terminal_id: &str) -> String {
    format!("group-{terminal_id}")
}

impl Terminals {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.groups
            .iter()
            .flat_map(|group| group.ids.iter().map(String::as_str))
    }

    pub fn count(&self) -> usize {
        self.ids().count()
    }

    /// The group the drawer shows: the active one, else the one holding the active terminal.
    pub fn visible(&self) -> Option<&Group> {
        self.groups
            .iter()
            .find(|group| group.id == self.active_group)
            .or_else(|| {
                self.groups
                    .iter()
                    .find(|group| group.ids.contains(&self.active))
            })
    }

    /// `hasReachedSplitLimit`.
    pub fn split_limit_reached(&self) -> bool {
        self.visible()
            .is_some_and(|group| group.ids.len() >= MAX_PER_GROUP)
    }

    /// Adds `id`; a split with no terminals is a new one, and a full group refuses it.
    pub fn add(&mut self, id: &str, placement: Placement) -> bool {
        if self.ids().any(|known| known == id) {
            return false;
        }
        let placement = if self.is_empty() {
            Placement::New
        } else {
            placement
        };
        match placement {
            Placement::New => {
                let group = self.unique_group_id(id);
                self.groups.push(Group {
                    id: group.clone(),
                    ids: vec![id.to_owned()],
                    vertical: false,
                });
                self.active_group = group;
            }
            Placement::Split { vertical } => {
                let Some(visible) = self.visible().map(|group| group.id.clone()) else {
                    return false;
                };
                let active = self.active.clone();
                let Some(destination) = self.groups.iter_mut().find(|group| group.id == visible)
                else {
                    return false;
                };
                if destination.ids.len() >= MAX_PER_GROUP {
                    return false;
                }
                match destination.ids.iter().position(|known| *known == active) {
                    Some(anchor) => destination.ids.insert(anchor + 1, id.to_owned()),
                    None => destination.ids.push(id.to_owned()),
                }
                destination.vertical = vertical;
                self.active_group = destination.id.clone();
            }
        }
        self.active = id.to_owned();
        true
    }

    fn unique_group_id(&self, terminal_id: &str) -> String {
        let base = group_id(terminal_id);
        let mut candidate = base.clone();
        let mut index = 2;
        while self.groups.iter().any(|group| group.id == candidate) {
            candidate = format!("{base}-{index}");
            index += 1;
        }
        candidate
    }

    pub fn activate(&mut self, id: &str) {
        if let Some(group) = self
            .groups
            .iter()
            .find(|group| group.ids.iter().any(|known| known == id))
        {
            self.active_group = group.id.clone();
            self.active = id.to_owned();
        }
    }

    /// `closeThreadTerminal`: when the active terminal goes, focus moves to its group neighbour
    /// (next, else previous), else to whichever terminal now sits at its old position.
    pub fn close(&mut self, id: &str) {
        let all: Vec<String> = self.ids().map(str::to_owned).collect();
        let Some(closed_index) = all.iter().position(|known| known == id) else {
            return;
        };
        let remaining: Vec<&String> = all.iter().filter(|known| *known != id).collect();
        if remaining.is_empty() {
            *self = Self::default();
            return;
        }
        let next_active = if self.active == id {
            let siblings = self
                .groups
                .iter()
                .find(|group| group.ids.iter().any(|known| known == id))
                .map(|group| group.ids.clone())
                .unwrap_or_default();
            let at = siblings.iter().position(|known| known == id).unwrap_or(0);
            siblings
                .get(at + 1)
                .or_else(|| at.checked_sub(1).and_then(|before| siblings.get(before)))
                .cloned()
                .unwrap_or_else(|| remaining[closed_index.min(remaining.len() - 1)].clone())
        } else {
            self.active.clone()
        };
        for group in &mut self.groups {
            group.ids.retain(|known| known != id);
        }
        self.groups.retain(|group| !group.ids.is_empty());
        self.active_group = self
            .groups
            .iter()
            .find(|group| group.ids.contains(&next_active))
            .or_else(|| self.groups.first())
            .map(|group| group.id.clone())
            .unwrap_or_default();
        self.active = next_active;
    }

    /// `nextTerminalId`: the lowest free number, with a unique suffix so it cannot reuse a session
    /// of the same name this client has not seen.
    pub fn next_id(&self, suffix: &str) -> String {
        let mut index = 1;
        while self.ids().any(|known| number(known) == Some(index)) {
            index += 1;
        }
        format!("term-{index}-{suffix}")
    }
}

/// The `N` of `term-N` or `terminal-N`, either with the uuid suffix `next_id` appends.
fn number(terminal_id: &str) -> Option<u64> {
    let rest = terminal_id
        .strip_prefix("terminal-")
        .or_else(|| terminal_id.strip_prefix("term-"))?;
    let (digits, suffix) = match rest.split_once('-') {
        Some((digits, suffix)) => (digits, Some(suffix)),
        None => (rest, None),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if suffix.is_some_and(|suffix| !is_uuid(suffix)) {
        return None;
    }
    digits.parse().ok()
}

fn is_uuid(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(part, len)| part.len() == len && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// `getTerminalLabel`.
pub fn label(terminal_id: &str) -> String {
    match number(terminal_id) {
        Some(number) => format!("Terminal {number}"),
        None => terminal_id.to_owned(),
    }
}

/// A random uuid in its textual form, for `next_id`'s suffix.
pub fn uuid_suffix() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "0123abcd-0123-0123-0123-0123456789ab";

    fn with(ids: &[&str]) -> Terminals {
        let mut terminals = Terminals::default();
        for id in ids {
            terminals.add(id, Placement::New);
        }
        terminals
    }

    #[test]
    fn a_split_joins_the_active_group_after_the_active_terminal() {
        let mut terminals = with(&["term-1", "term-2"]);
        terminals.activate("term-1");
        assert!(terminals.add("term-3", Placement::Split { vertical: true }));
        let group = &terminals.groups[0];
        assert_eq!(group.ids, ["term-1", "term-3"]);
        assert!(group.vertical);
        assert_eq!(terminals.active, "term-3");
        assert_eq!(terminals.groups.len(), 2);
    }

    #[test]
    fn a_group_holds_at_most_four() {
        let mut terminals = with(&["term-1"]);
        for id in ["term-2", "term-3", "term-4"] {
            assert!(terminals.add(id, Placement::Split { vertical: false }));
        }
        assert!(terminals.split_limit_reached());
        assert!(!terminals.add("term-5", Placement::Split { vertical: false }));
        assert_eq!(terminals.count(), 4);
        assert!(terminals.add("term-5", Placement::New));
    }

    #[test]
    fn closing_the_active_terminal_moves_to_its_group_neighbour() {
        let mut terminals = with(&["term-1"]);
        terminals.add("term-2", Placement::Split { vertical: false });
        terminals.add("term-3", Placement::Split { vertical: false });
        terminals.activate("term-2");
        terminals.close("term-2");
        assert_eq!(terminals.active, "term-3");
        terminals.close("term-3");
        assert_eq!(terminals.active, "term-1");
        terminals.close("term-1");
        assert!(terminals.is_empty());
    }

    #[test]
    fn closing_a_whole_group_falls_back_by_position() {
        let mut terminals = with(&["term-1", "term-2", "term-3"]);
        terminals.close("term-3");
        assert_eq!(terminals.active, "term-2");
        assert_eq!(terminals.active_group, "group-term-2");
        terminals.close("term-1");
        assert_eq!(terminals.active, "term-2");
    }

    #[test]
    fn ids_take_the_lowest_free_number_and_label_without_the_suffix() {
        let terminals = with(&["term-1", &format!("term-3-{UUID}")]);
        assert_eq!(terminals.next_id("x"), "term-2-x");
        assert_eq!(label("term-1"), "Terminal 1");
        assert_eq!(label(&format!("term-3-{UUID}")), "Terminal 3");
        assert_eq!(label("build"), "build");
        assert!(is_uuid(&uuid_suffix()));
    }
}
