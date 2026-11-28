//! The model picker's derived state, ported from `ModelPickerContent.tsx`, `modelPickerSearch.ts`
//! and `@t3tools/shared/searchRanking`. The view only lays out what these functions return.

use crate::providers::{PickerInstance, PickerModel};

/// `MODEL_PICKER_JUMP_KEYBINDING_COMMANDS`: the first nine usable rows take Cmd+1 to Cmd+9.
pub const JUMP_KEYS: usize = 9;

const FAVORITE_BOOST: i32 = 24;
const EXACT_NAME: i32 = -1000;

/// A rail entry of the picker's sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    Favorites,
    Instance(String),
}

/// Which rail entry the last model was picked from, so the picker reopens there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Favorites,
    Instance,
}

/// The column that takes the arrow keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    List,
    Sidebar,
    Effort,
}

/// The open picker's view state: the shown rail entry, the highlighted item of the focused
/// column, and the entry the last model came from.
#[derive(Debug, Default)]
pub struct State {
    pub tab: Option<Tab>,
    pub expanded: bool,
    pub highlight: usize,
    pub focus: Focus,
    pub effort: usize,
    pub origin: Option<Origin>,
}

/// `initialModelPickerProvider` without the locked-provider cases, which the desktop has none of.
pub fn initial_tab(
    active_instance: &str,
    active_visible: bool,
    has_favorites: bool,
    last_origin: Option<Origin>,
) -> Tab {
    if !has_favorites {
        return Tab::Instance(active_instance.to_owned());
    }
    if !active_visible || last_origin == Some(Origin::Favorites) {
        Tab::Favorites
    } else {
        Tab::Instance(active_instance.to_owned())
    }
}

/// One listed model with the instance it runs on.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub instance: &'a PickerInstance,
    pub model: &'a PickerModel,
}

impl Row<'_> {
    pub fn usable(&self) -> bool {
        self.instance.unavailable.is_none()
    }
}

/// What the model column lists.
#[derive(Debug)]
pub struct Listing<'a> {
    pub rows: Vec<Row<'a>>,
    /// The trailing "Show more" / "Show less" item: `Some(expanded)` when older models exist.
    pub older: Option<bool>,
}

impl Listing<'_> {
    /// Navigable items: every row, plus the older-models toggle.
    pub fn items(&self) -> usize {
        self.rows.len() + usize::from(self.older.is_some())
    }

    /// Row indexes that take Cmd+1.. in order.
    pub fn jump_rows(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.usable())
            .map(|(index, _)| index)
            .take(JUMP_KEYS)
            .collect()
    }
}

/// The rows for `tab`, or the ranked matches of `query` across every instance. Starred models lead
/// an instance's list, and a collapsed instance hides its older models behind the toggle.
pub fn listing<'a>(
    instances: &'a [PickerInstance],
    tab: &Tab,
    query: &str,
    expanded: bool,
) -> Listing<'a> {
    if !query.trim().is_empty() {
        return Listing {
            rows: rank_models(instances, query),
            older: None,
        };
    }
    let all = |instance: &'a PickerInstance| {
        instance
            .models
            .iter()
            .map(move |model| Row { instance, model })
    };
    match tab {
        Tab::Favorites => Listing {
            rows: instances
                .iter()
                .flat_map(all)
                .filter(|row| row.model.favorite)
                .collect(),
            older: None,
        },
        Tab::Instance(id) => {
            let mut rows: Vec<Row> = instances
                .iter()
                .filter(|instance| &instance.instance_id == id)
                .flat_map(all)
                .collect();
            rows.sort_by_key(|row| !row.model.favorite);
            let (older, mut current): (Vec<Row>, Vec<Row>) = rows
                .into_iter()
                .partition(|row| row.model.legacy && !row.model.favorite);
            let toggle = (!older.is_empty()).then_some(expanded);
            if expanded {
                current.extend(older);
            }
            Listing {
                rows: current,
                older: toggle,
            }
        }
    }
}

/// Whether the older models start expanded: when the active model is one of them.
pub fn starts_expanded(instances: &[PickerInstance], instance_id: &str, slug: &str) -> bool {
    instances
        .iter()
        .filter(|instance| instance.instance_id == instance_id)
        .flat_map(|instance| &instance.models)
        .any(|model| model.slug == slug && model.legacy && !model.favorite)
}

/// The rail entry after `current` in `direction` (+1 or -1), wrapping, over Favorites and the
/// usable instances (`adjacentModelPickerProvider`).
pub fn adjacent_tab(instances: &[PickerInstance], current: &Tab, direction: isize) -> Tab {
    let tabs: Vec<Tab> = std::iter::once(Tab::Favorites)
        .chain(
            instances
                .iter()
                .filter(|instance| instance.unavailable.is_none())
                .map(|instance| Tab::Instance(instance.instance_id.clone())),
        )
        .collect();
    match tabs.iter().position(|tab| tab == current) {
        Some(index) => tabs[step(index, tabs.len(), direction)].clone(),
        None if direction > 0 => tabs[0].clone(),
        None => tabs[tabs.len() - 1].clone(),
    }
}

/// The item after `current` in `delta` steps, wrapping around `count` items.
pub fn step(current: usize, count: usize, delta: isize) -> usize {
    if count == 0 {
        return 0;
    }
    (current as isize + delta).rem_euclid(count as isize) as usize
}

/// The picker's search: every token must match a field; exact names first, then the tightest
/// matches with starred models ahead, then list order (`rankModelPickerSearch`).
pub fn rank_models<'a>(instances: &'a [PickerInstance], query: &str) -> Vec<Row<'a>> {
    let mut ranked: Vec<(i32, bool, usize, Row<'a>)> = Vec::new();
    let rows = instances.iter().flat_map(|instance| {
        instance
            .models
            .iter()
            .map(move |model| Row { instance, model })
    });
    for (order, row) in rows.enumerate() {
        if let Some(score) = score(row, query) {
            ranked.push((score, row.model.favorite, order, row));
        }
    }
    ranked.sort_by_key(|(score, favorite, order, _)| (*score, !*favorite, *order));
    ranked.into_iter().map(|(.., row)| row).collect()
}

fn score(row: Row, query: &str) -> Option<i32> {
    let model = row.model;
    let normalize = |text: &str| text.trim().to_lowercase();
    let tokens: Vec<String> = normalize(query)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if tokens.is_empty() {
        return Some(0);
    }
    let query = tokens.join(" ");
    let collapse = |text: &str| {
        normalize(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    if query == collapse(&model.name)
        || model.short_name.as_deref().map(collapse) == Some(query.clone())
    {
        return Some(EXACT_NAME);
    }
    let parts: Vec<&str> = [
        Some(model.name.as_str()),
        model.short_name.as_deref(),
        model.sub_provider.as_deref(),
        Some(row.instance.driver.as_str()),
        Some(row.instance.name.as_str()),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect();
    let joined = normalize(&parts.join(" "));
    let fields: Vec<String> = parts
        .iter()
        .map(|part| normalize(part))
        .chain(std::iter::once(joined))
        .collect();
    let mut total = 0;
    for token in &tokens {
        total += fields
            .iter()
            .enumerate()
            .filter_map(|(index, field)| token_score(field, token, index as i32 * 10))
            .min()?;
    }
    Some(if model.favorite {
        total - FAVORITE_BOOST
    } else {
        total
    })
}

/// `scoreQueryMatch` at a field's base, position-insensitive, with the fuzzy tier for tokens of
/// three or more characters.
fn token_score(field: &str, token: &str, base: i32) -> Option<i32> {
    if field.is_empty() {
        return None;
    }
    if field == token {
        return Some(base);
    }
    if field.starts_with(token) {
        return Some(base + 2);
    }
    if [" ", "-", "_", "/"]
        .iter()
        .any(|marker| field.contains(&format!("{marker}{token}")))
    {
        return Some(base + 4);
    }
    if field.contains(token) {
        return Some(base + 6);
    }
    if token.chars().count() >= 3 {
        return subsequence_penalty(field, token).map(|penalty| base + 100 + penalty);
    }
    None
}

/// `scoreSubsequenceMatch` with `ignorePosition`: gaps between matched characters and the span.
fn subsequence_penalty(value: &str, query: &str) -> Option<i32> {
    let wanted: Vec<char> = query.chars().collect();
    let mut next = 0;
    let (mut first, mut previous) = (None, None);
    let mut gaps = 0;
    for (index, ch) in value.chars().enumerate() {
        if ch != wanted[next] {
            continue;
        }
        first.get_or_insert(index);
        if let Some(previous) = previous {
            gaps += (index - previous - 1) as i32;
        }
        previous = Some(index);
        next += 1;
        if next == wanted.len() {
            let span = (index - first? + 1) as i32 - wanted.len() as i32;
            return Some(gaps * 3 + span);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Mark;

    fn instance(id: &str, driver: &str, name: &str, models: Vec<PickerModel>) -> PickerInstance {
        PickerInstance {
            instance_id: id.to_owned(),
            mark: Mark {
                driver: driver.to_owned(),
                display_name: name.to_owned(),
                badge: None,
            },
            name: name.to_owned(),
            unavailable: None,
            models,
            driver: driver.to_owned(),
        }
    }

    fn model(name: &str) -> PickerModel {
        PickerModel {
            slug: name.to_lowercase().replace(' ', "-"),
            label: name.to_owned(),
            title: name.to_owned(),
            name: name.to_owned(),
            short_name: None,
            sub_provider: None,
            favorite: false,
            legacy: false,
            new: false,
        }
    }

    fn catalog() -> Vec<PickerInstance> {
        let names = |names: &[&str]| names.iter().map(|name| model(name)).collect::<Vec<_>>();
        vec![
            instance(
                "claudeAgent",
                "claudeAgent",
                "Claude",
                names(&[
                    "Claude Opus 5.5",
                    "Claude Sonnet 5.5",
                    "Claude Opus 5",
                    "Claude Opus 4.8",
                    "Claude Opus 4.7",
                    "Claude Opus 4.6",
                    "Claude Opus 4.5",
                ]),
            ),
            instance(
                "codex",
                "codex",
                "Codex",
                names(&["GPT-6.1 Sol", "GPT-6 Astra", "GPT-6 Sol", "GPT-5.5"]),
            ),
            instance(
                "opencode",
                "opencode",
                "OpenCode",
                ["Kimi K3", "Kimi K2.5", "Kimi K2"]
                    .iter()
                    .map(|name| PickerModel {
                        sub_provider: Some("Moonshot".to_owned()),
                        ..model(name)
                    })
                    .collect(),
            ),
        ]
    }

    fn search(instances: &[PickerInstance], query: &str) -> Vec<String> {
        rank_models(instances, query)
            .iter()
            .map(|row| row.model.name.clone())
            .collect()
    }

    #[test]
    fn a_version_token_keeps_the_newest_first() {
        let instances = catalog();
        assert_eq!(
            search(&instances, "opus 4"),
            [
                "Claude Opus 4.8",
                "Claude Opus 4.7",
                "Claude Opus 4.6",
                "Claude Opus 4.5"
            ]
        );
        assert_eq!(search(&instances, "opus 5."), ["Claude Opus 5.5"]);
    }

    #[test]
    fn an_exact_name_outranks_models_that_extend_it() {
        let instances = catalog();
        assert_eq!(
            search(&instances, "gpt-6 sol")[..2],
            ["GPT-6 Sol", "GPT-6.1 Sol"]
        );
        assert_eq!(
            search(&instances, "claude opus 5")[..2],
            ["Claude Opus 5", "Claude Opus 5.5"]
        );
    }

    #[test]
    fn the_provider_matches_when_the_name_does_not() {
        assert_eq!(
            search(&catalog(), "moonshot"),
            ["Kimi K3", "Kimi K2.5", "Kimi K2"]
        );
    }

    #[test]
    fn a_favorite_leads_its_family_but_not_clearly_better_matches() {
        let mut instances = catalog();
        instances[0].models[6].favorite = true;
        assert_eq!(
            search(&instances, "opus")[..3],
            ["Claude Opus 4.5", "Claude Opus 5.5", "Claude Opus 5"]
        );
        let exact = score(
            Row {
                instance: &instance("cursor", "cursor", "Cursor", vec![]),
                model: &model("Opus 4.7"),
            },
            "opus 4.7",
        )
        .unwrap();
        let favorite = score(
            Row {
                instance: &instances[0],
                model: &PickerModel {
                    favorite: true,
                    ..model("Claude Opus 4.7")
                },
            },
            "opus 4.7",
        )
        .unwrap();
        assert!(exact < favorite);
    }

    #[test]
    fn fuzzy_matches_need_three_characters_and_rank_below_plain_ones() {
        let instances = catalog();
        assert!(search(&instances, "kk").is_empty());
        assert_eq!(search(&instances, "kmi")[0], "Kimi K3");
        let plain = score(rank_models(&instances, "kimi")[0], "kimi").unwrap();
        let fuzzy = score(rank_models(&instances, "kmi")[0], "kmi").unwrap();
        assert!(plain < fuzzy);
        assert!(search(&instances, "zzz").is_empty());
    }

    #[test]
    fn reopens_on_the_tab_the_last_model_came_from() {
        let active = "claudeAgent_personal";
        let instance = Tab::Instance(active.to_owned());
        assert_eq!(
            initial_tab(active, true, true, Some(Origin::Favorites)),
            Tab::Favorites
        );
        assert_eq!(
            initial_tab(active, true, true, Some(Origin::Instance)),
            instance
        );
        assert_eq!(initial_tab(active, true, true, None), instance);
        assert_eq!(
            initial_tab(active, true, false, Some(Origin::Favorites)),
            instance
        );
        assert_eq!(
            initial_tab(active, false, true, Some(Origin::Instance)),
            Tab::Favorites
        );
    }

    #[test]
    fn favorites_lead_and_older_models_hide_behind_the_toggle() {
        let mut instances = catalog();
        instances[0].models[3].legacy = true;
        instances[0].models[4].legacy = true;
        instances[0].models[4].favorite = true;
        let tab = Tab::Instance("claudeAgent".to_owned());
        let names = |listing: &Listing| {
            listing
                .rows
                .iter()
                .map(|row| row.model.name.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        let collapsed = listing(&instances, &tab, "", false);
        assert_eq!(collapsed.older, Some(false));
        assert_eq!(
            names(&collapsed),
            [
                "Claude Opus 4.7",
                "Claude Opus 5.5",
                "Claude Sonnet 5.5",
                "Claude Opus 5",
                "Claude Opus 4.6",
                "Claude Opus 4.5"
            ]
        );
        assert_eq!(collapsed.items(), 7);
        let expanded = listing(&instances, &tab, "", true);
        assert_eq!(expanded.older, Some(true));
        assert_eq!(names(&expanded).last().unwrap(), "Claude Opus 4.8");
        assert!(starts_expanded(
            &instances,
            "claudeAgent",
            "claude-opus-4.8"
        ));
        assert!(!starts_expanded(
            &instances,
            "claudeAgent",
            "claude-opus-4.7"
        ));
    }

    #[test]
    fn the_favorites_tab_lists_stars_across_instances_and_search_ignores_tabs() {
        let mut instances = catalog();
        instances[1].models[0].favorite = true;
        instances[0].models[1].favorite = true;
        let favorites = listing(&instances, &Tab::Favorites, "", false);
        let names: Vec<&str> = favorites
            .rows
            .iter()
            .map(|row| row.model.name.as_str())
            .collect();
        assert_eq!(names, ["Claude Sonnet 5.5", "GPT-6.1 Sol"]);
        assert_eq!(favorites.older, None);
        let searched = listing(&instances, &Tab::Favorites, "kimi", false);
        assert_eq!(searched.rows.len(), 3);
    }

    #[test]
    fn jump_keys_number_the_first_nine_usable_rows() {
        let mut instances = catalog();
        instances[0].unavailable = Some("Claude: Unavailable.".to_owned());
        let all = listing(&instances, &Tab::Favorites, "o", false);
        let jumps = all.jump_rows();
        assert!(jumps.len() <= JUMP_KEYS);
        assert!(jumps.iter().all(|index| all.rows[*index].usable()));
        let codex = listing(&instances, &Tab::Instance("codex".to_owned()), "", false);
        assert_eq!(codex.jump_rows(), [0, 1, 2, 3]);
    }

    #[test]
    fn the_rail_and_the_list_wrap_around() {
        let mut instances = catalog();
        instances[2].unavailable = Some("OpenCode: Unavailable.".to_owned());
        let claude = Tab::Instance("claudeAgent".to_owned());
        assert_eq!(
            adjacent_tab(&instances, &Tab::Favorites, -1),
            Tab::Instance("codex".to_owned())
        );
        assert_eq!(
            adjacent_tab(&instances, &claude, 1),
            Tab::Instance("codex".to_owned())
        );
        assert_eq!(
            adjacent_tab(&instances, &Tab::Instance("codex".to_owned()), 1),
            Tab::Favorites
        );
        assert_eq!(
            adjacent_tab(&instances, &Tab::Instance("gone".to_owned()), 1),
            Tab::Favorites
        );
        assert_eq!(step(0, 3, -1), 2);
        assert_eq!(step(2, 3, 1), 0);
        assert_eq!(step(0, 0, 1), 0);
    }
}
