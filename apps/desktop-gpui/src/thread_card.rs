//! What the sidebar thread's hover card lists beyond the row itself: the pull request lines of
//! `ThreadPullRequestsMiniList` and the "Handed off from" names of `SidebarThreadTooltip`.

use crate::shell::timestamp_ms;
use crate::status::PrState;
use crate::wire::generated::{
    PullRequestsItem, PullRequestsItemSource as LinkSource, SnapshotState,
};
use std::collections::{HashMap, HashSet};

/// How a chain of pull requests came to be: the host's own stack, or one derived from
/// head and base branches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainKind {
    Native,
    Derived,
}

/// One `ThreadPullRequestMiniListItem`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrLine {
    pub number: u64,
    /// `Pending` while the link has no snapshot yet; it shows the plain pull request glyph.
    pub state: PrState,
    /// The snapshot's title, else the repository.
    pub title: String,
    pub depth: usize,
    /// On a chain's first line: its kind and how many pull requests it holds.
    pub stack: Option<(ChainKind, usize)>,
}

struct Chain {
    kind: ChainKind,
    layers: Vec<usize>,
    depths: Vec<usize>,
}

fn repository_key(link: &PullRequestsItem) -> String {
    format!(
        "{}/{}",
        link.host.trim().to_lowercase(),
        link.repository.trim().to_lowercase()
    )
}

fn key_of(link: &PullRequestsItem) -> String {
    format!("{}#{}", repository_key(link), link.number)
}

fn latest_updated_at(link: &PullRequestsItem) -> i64 {
    let value = link
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.updated_at.as_deref())
        .unwrap_or(&link.linked_at);
    timestamp_ms(value).unwrap_or(0)
}

/// `resolveThreadPullRequestChains` over the visible links.
fn chains(visible: &[&PullRequestsItem]) -> Vec<Chain> {
    let mut chains = Vec::new();
    let mut placed: HashSet<String> = HashSet::new();

    let mut stacks: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, link) in visible.iter().enumerate() {
        let Some(stack) = &link.stack else { continue };
        let key = format!("{}#stack:{}", repository_key(link), stack.id);
        match stacks.iter_mut().find(|(known, _)| *known == key) {
            Some((_, members)) => members.push(index),
            None => stacks.push((key, vec![index])),
        }
    }
    for (_, mut members) in stacks {
        // Saved snapshots can lag behind newer members of the same stack.
        let numbers: HashSet<u64> = members.iter().map(|&i| visible[i].number.get()).collect();
        let layers_of = |i: usize| &visible[i].stack.as_ref().expect("stack member").layers;
        let coverage = |layers: &[crate::wire::generated::StackLayersItem]| {
            layers
                .iter()
                .filter(|layer| numbers.contains(&layer.number.get()))
                .count()
        };
        let mut layers = layers_of(members[0]);
        let mut best = coverage(layers);
        for &member in &members[1..] {
            let candidate = layers_of(member);
            let covered = coverage(candidate);
            if covered > best || (covered == best && candidate.len() > layers.len()) {
                layers = candidate;
                best = covered;
            }
        }
        let order: HashMap<u64, usize> = layers
            .iter()
            .enumerate()
            .map(|(index, layer)| (layer.number.get(), index))
            .collect();
        members.sort_by_key(|&i| {
            order
                .get(&visible[i].number.get())
                .copied()
                .unwrap_or(layers.len())
        });
        placed.extend(members.iter().map(|&i| key_of(visible[i])));
        chains.push(Chain {
            kind: ChainKind::Native,
            depths: (0..members.len()).collect(),
            layers: members,
        });
    }

    let remaining: Vec<usize> = (0..visible.len())
        .filter(|&i| !placed.contains(&key_of(visible[i])))
        .collect();
    let branch_key = |i: usize, branch: &str| format!("{}:{branch}", repository_key(visible[i]));
    // A head branch shared by several pull requests names no single parent.
    let mut by_head: HashMap<String, Option<usize>> = HashMap::new();
    for &i in &remaining {
        let Some(snapshot) = &visible[i].snapshot else {
            continue;
        };
        let key = branch_key(i, &snapshot.head_branch);
        let duplicate = by_head.contains_key(&key);
        by_head.insert(key, if duplicate { None } else { Some(i) });
    }
    let parent_of = |i: usize| -> Option<usize> {
        let snapshot = visible[i].snapshot.as_ref()?;
        by_head
            .get(&branch_key(i, &snapshot.base_branch))
            .copied()
            .flatten()
            .filter(|&parent| parent != i)
    };
    let mut children: HashMap<String, Vec<usize>> = HashMap::new();
    for &i in &remaining {
        if let Some(parent) = parent_of(i) {
            children.entry(key_of(visible[parent])).or_default().push(i);
        }
    }
    for &base in &remaining {
        if parent_of(base).is_some() {
            continue;
        }
        let (mut layers, mut depths) = (Vec::new(), Vec::new());
        let mut pending = vec![(base, 0usize)];
        while let Some((next, depth)) = pending.pop() {
            let key = key_of(visible[next]);
            // Two links with one identity would otherwise share a child list and expand forever.
            if !placed.insert(key.clone()) {
                continue;
            }
            layers.push(next);
            depths.push(depth);
            for &child in children.get(&key).into_iter().flatten().rev() {
                pending.push((child, depth + 1));
            }
        }
        if !layers.is_empty() {
            chains.push(Chain {
                kind: ChainKind::Derived,
                layers,
                depths,
            });
        }
    }
    for &i in &remaining {
        if !placed.contains(&key_of(visible[i])) {
            chains.push(Chain {
                kind: ChainKind::Derived,
                layers: vec![i],
                depths: vec![0],
            });
        }
    }
    chains
}

fn state_of(link: &PullRequestsItem) -> PrState {
    match &link.snapshot {
        None => PrState::Pending,
        Some(snapshot) => match snapshot.state {
            SnapshotState::Open if snapshot.is_draft => PrState::Draft,
            SnapshotState::Open => PrState::Open,
            SnapshotState::Closed => PrState::Closed,
            SnapshotState::Merged => PrState::Merged,
        },
    }
}

/// `visibleThreadPullRequests`, `resolveThreadPullRequestChains` and `pullRequestListLines`:
/// chains newest first, each from its base up, indented by depth.
pub fn pr_lines(links: &[PullRequestsItem]) -> Vec<PrLine> {
    let visible: Vec<&PullRequestsItem> = links
        .iter()
        .filter(|link| link.source != LinkSource::StackDismissed)
        .collect();
    let mut chains = chains(&visible);
    let newest = |chain: &Chain| {
        chain
            .layers
            .iter()
            .map(|&i| latest_updated_at(visible[i]))
            .max()
            .unwrap_or(0)
    };
    chains.sort_by_key(|chain| std::cmp::Reverse(newest(chain)));
    chains
        .iter()
        .flat_map(|chain| {
            let visible = &visible;
            chain.layers.iter().enumerate().map(move |(position, &i)| {
                let link = visible[i];
                PrLine {
                    number: link.number.get(),
                    state: state_of(link),
                    title: link
                        .snapshot
                        .as_ref()
                        .map_or_else(|| link.repository.clone(), |s| s.title.clone()),
                    depth: chain.depths[position],
                    stack: (position == 0 && chain.layers.len() > 1)
                        .then_some((chain.kind, chain.layers.len())),
                }
            })
        })
        .collect()
}

/// The instances a thread moved off, as the card names them: its history without the current
/// instance, each by display name or, unknown, by id.
pub fn previous_providers(
    history: &[String],
    current: &str,
    display_name: impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    history
        .iter()
        .filter(|id| id.as_str() != current)
        .map(|id| display_name(id).unwrap_or_else(|| id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn link(number: u64, head: &str, base: &str, updated: &str) -> Value {
        json!({
            "host": "github.com", "linkedAt": "2026-01-01T00:00:00.000Z",
            "number": number, "repository": "o/r", "source": "agent",
            "url": format!("https://github.com/o/r/pull/{number}"),
            "snapshot": {
                "baseBranch": base, "headBranch": head, "isDraft": false, "state": "open",
                "syncedAt": "2026-01-01T00:00:00.000Z", "title": format!("PR {number}"),
                "updatedAt": updated,
            },
            "stack": null,
        })
    }

    fn lines(links: Vec<Value>) -> Vec<PrLine> {
        pr_lines(&serde_json::from_value::<Vec<PullRequestsItem>>(links.into()).expect("links"))
    }

    fn shape(lines: &[PrLine]) -> Vec<(u64, usize)> {
        lines.iter().map(|l| (l.number, l.depth)).collect()
    }

    #[test]
    fn branches_chain_by_head_and_base_with_depth() {
        let result = lines(vec![
            link(3, "c", "b", "2026-01-03T00:00:00.000Z"),
            link(1, "a", "main", "2026-01-03T00:00:00.000Z"),
            link(2, "b", "a", "2026-01-03T00:00:00.000Z"),
        ]);
        assert_eq!(shape(&result), [(1, 0), (2, 1), (3, 2)]);
        assert_eq!(result[0].stack, Some((ChainKind::Derived, 3)));
        assert_eq!(result[1].stack, None);
    }

    #[test]
    fn chains_list_the_most_recently_updated_first() {
        let result = lines(vec![
            link(1, "a", "main", "2026-01-01T00:00:00.000Z"),
            link(2, "x", "main", "2026-02-01T00:00:00.000Z"),
        ]);
        assert_eq!(shape(&result), [(2, 0), (1, 0)]);
        assert!(result.iter().all(|line| line.stack.is_none()));
    }

    #[test]
    fn native_stacks_follow_the_stack_layers_and_dismissed_links_hide() {
        let layers = json!([
            {"headBranch": "a", "number": 5, "state": "open"},
            {"headBranch": "b", "number": 4, "state": "open"},
        ]);
        let stacked = |number: u64| {
            let mut value = link(number, "h", "main", "2026-01-01T00:00:00.000Z");
            value["stack"] = json!({
                "base": "main", "id": "s", "kind": "native", "layers": layers,
                "number": 5, "url": "https://github.com/o/r/pull/5",
            });
            value
        };
        let mut dismissed = link(9, "z", "main", "2026-03-01T00:00:00.000Z");
        dismissed["source"] = "stack-dismissed".into();
        let result = lines(vec![stacked(4), stacked(5), dismissed]);
        assert_eq!(shape(&result), [(5, 0), (4, 1)]);
        assert_eq!(result[0].stack, Some((ChainKind::Native, 2)));
    }

    #[test]
    fn a_link_without_a_snapshot_is_pending_and_titled_by_its_repository() {
        let mut bare = link(7, "a", "main", "2026-01-01T00:00:00.000Z");
        bare["snapshot"] = Value::Null;
        let result = lines(vec![bare]);
        assert_eq!(result[0].state, PrState::Pending);
        assert_eq!(result[0].title, "o/r");
    }

    #[test]
    fn handed_off_names_skip_the_current_instance() {
        let history = ["codex", "claude_work", "mystery"].map(str::to_owned);
        let names = previous_providers(&history, "codex", |id| {
            (id == "claude_work").then(|| "Work".to_owned())
        });
        assert_eq!(names, ["Work", "mystery"]);
        assert_eq!(names.join(", "), "Work, mystery");
    }
}
