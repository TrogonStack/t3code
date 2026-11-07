//! The branch ref picker of the thread details card: `BranchPicker.tsx` and
//! `BranchToolbar.logic.ts` over `vcs.listRefs`, with `splitForMiddleTruncate` from
//! `ui/middle-truncate.tsx`.

use serde_json::Value;

/// One `VcsRef` of a `vcs.listRefs` page.
#[derive(Clone, Debug, PartialEq)]
pub struct Ref {
    pub name: String,
    pub is_remote: bool,
    pub current: bool,
    pub is_default: bool,
    pub worktree_path: Option<String>,
}

/// The right-side tag of a picker row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Current,
    Worktree,
    Remote,
    Default,
}

impl Tag {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Worktree => "worktree",
            Self::Remote => "remote",
            Self::Default => "default",
        }
    }
}

/// One page of refs and the counts the footer reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub refs: Vec<Ref>,
    pub total: usize,
    pub has_more: bool,
}

impl Page {
    pub fn decode(value: &Value) -> Self {
        let refs: Vec<Ref> = value["refs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                Some(Ref {
                    name: entry["name"].as_str()?.to_owned(),
                    is_remote: entry["isRemote"].as_bool().unwrap_or(false),
                    current: entry["current"].as_bool().unwrap_or(false),
                    is_default: entry["isDefault"].as_bool().unwrap_or(false),
                    worktree_path: entry["worktreePath"].as_str().map(str::to_owned),
                })
            })
            .collect();
        let total = value["totalCount"]
            .as_u64()
            .map_or(refs.len(), |count| count as usize);
        Self {
            has_more: !value["nextCursor"].is_null(),
            refs,
            total,
        }
    }

    /// The picker's footer: the loading states, or how much of the list is shown.
    pub fn status(&self, loading: bool) -> Option<String> {
        if loading {
            return Some(if self.refs.is_empty() {
                "Loading refs...".to_owned()
            } else {
                "Loading more refs...".to_owned()
            });
        }
        self.has_more
            .then(|| format!("Showing {} of {} refs", self.refs.len(), self.total))
    }
}

/// The one tag a row shows. A worktree only counts when it is not the project's own checkout.
pub fn tag(entry: &Ref, project_cwd: &str) -> Option<Tag> {
    if entry.current {
        Some(Tag::Current)
    } else if entry
        .worktree_path
        .as_deref()
        .is_some_and(|path| path != project_cwd)
    {
        Some(Tag::Worktree)
    } else if entry.is_remote {
        Some(Tag::Remote)
    } else if entry.is_default {
        Some(Tag::Default)
    } else {
        None
    }
}

/// `sanitizeNewRefName`: spaces in a typed name become dashes.
pub fn sanitize_new_ref_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join("-")
}

/// `deriveLocalBranchNameFromRemoteRef`: `origin/feature/x` is the local `feature/x`.
pub fn local_name_of_remote(name: &str) -> &str {
    name.split_once('/').map_or(name, |(_, rest)| rest)
}

/// `shouldIncludeBranchPickerItem` for a ref: the query matches as typed or as a ref name.
pub fn matches_query(name: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let name = name.to_lowercase();
    name.contains(&query) || name.contains(&sanitize_new_ref_name(&query))
}

const DEFAULT_TAIL: usize = 10;
const MAX_SEGMENT_TAIL: usize = 16;
const MIN_WORD_TAIL: usize = 4;

fn is_word_break(ch: char) -> bool {
    matches!(ch, '-' | '_' | '.' | '/')
}

/// `splitForMiddleTruncate`: the head that may be cut and the tail that stays, or `None` for a
/// value too short to truncate.
pub fn split_for_middle_truncate(value: &str, tail: Option<usize>) -> Option<(String, String)> {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    let mut keep = tail.unwrap_or(DEFAULT_TAIL);
    if tail.is_none()
        && let Some(slash) = chars.iter().rposition(|ch| *ch == '/')
        && slash > 0
        && slash < len - 1
    {
        let segment = len - slash - 1;
        keep = if segment <= MAX_SEGMENT_TAIL {
            segment
        } else {
            DEFAULT_TAIL
        };
    }
    if keep == 0 || len <= keep + 4 {
        return None;
    }
    let mut cut = len - keep;
    if tail.is_none() && !is_word_break(chars[cut - 1]) {
        let starts_word = |index: usize| is_word_break(chars[index - 1]);
        let mut snapped = (cut + 1..=len - MIN_WORD_TAIL).find(|index| starts_word(*index));
        if snapped.is_none() {
            snapped = (5..cut)
                .rev()
                .take_while(|index| *index + MAX_SEGMENT_TAIL >= len)
                .find(|index| starts_word(*index));
        }
        if let Some(index) = snapped.filter(|index| *index > 0) {
            cut = index;
        }
    }
    Some((chars[..cut].iter().collect(), chars[cut..].iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn split(value: &str, tail: Option<usize>) -> Option<(String, String)> {
        split_for_middle_truncate(value, tail)
    }

    fn tail_of(value: &str) -> String {
        split(value, None).expect("split").1
    }

    #[test]
    fn short_values_are_left_whole() {
        assert_eq!(split("main", None), None);
        assert_eq!(split("feature", None), None);
    }

    #[test]
    fn the_last_path_segment_stays() {
        assert_eq!(
            split("feature/ui-polish", None),
            Some(("feature/".to_owned(), "ui-polish".to_owned()))
        );
    }

    #[test]
    fn automatic_tails_start_on_a_word() {
        assert_eq!(tail_of("t3/clone-repo-to-other-machine"), "machine");
        assert_eq!(tail_of("t3/investigate-release-build-times"), "times");
        assert_eq!(
            tail_of("t3/compare-long-featurenames-ab"),
            "featurenames-ab"
        );
        assert_eq!(tail_of("t3/abcdefghijklmnopqrstuvwxyz"), "qrstuvwxyz");
    }

    #[test]
    fn an_explicit_tail_is_exact() {
        assert_eq!(
            split("t3/clone-repo-to-other-machine", Some(6)).map(|pair| pair.1),
            Some("achine".to_owned())
        );
    }

    #[test]
    fn truncation_counts_characters() {
        let value = "ramas/ñandú-ñandú-ñandú-ñandú";
        let (head, tail) = split(value, Some(5)).expect("split");
        assert_eq!(format!("{head}{tail}"), value);
        assert_eq!(tail.chars().count(), 5);
    }

    #[test]
    fn tags_follow_the_web_priority() {
        let base = Ref {
            name: "main".to_owned(),
            is_remote: false,
            current: false,
            is_default: false,
            worktree_path: None,
        };
        let with = |edit: fn(&mut Ref)| {
            let mut entry = base.clone();
            edit(&mut entry);
            tag(&entry, "/repo")
        };
        assert_eq!(with(|_| {}), None);
        assert_eq!(with(|r| r.is_default = true), Some(Tag::Default));
        assert_eq!(
            with(|r| {
                r.is_default = true;
                r.is_remote = true;
            }),
            Some(Tag::Remote)
        );
        assert_eq!(
            with(|r| {
                r.is_remote = true;
                r.worktree_path = Some("/elsewhere".to_owned());
            }),
            Some(Tag::Worktree)
        );
        assert_eq!(with(|r| r.worktree_path = Some("/repo".to_owned())), None);
        assert_eq!(
            with(|r| {
                r.current = true;
                r.worktree_path = Some("/elsewhere".to_owned());
            }),
            Some(Tag::Current)
        );
    }

    #[test]
    fn queries_match_as_typed_or_sanitized() {
        assert!(matches_query("feature/Ui-Polish", "ui-pol"));
        assert!(matches_query("feature/ui-polish", "ui polish"));
        assert!(matches_query("anything", "  "));
        assert!(!matches_query("main", "dev"));
    }

    #[test]
    fn names_derive_from_remote_refs() {
        assert_eq!(local_name_of_remote("origin/feature/x"), "feature/x");
        assert_eq!(local_name_of_remote("main"), "main");
        assert_eq!(sanitize_new_ref_name("  my  new branch "), "my-new-branch");
    }

    #[test]
    fn pages_decode_and_report_their_footer() {
        let page = Page::decode(&json!({
            "refs": [
                {"name": "main", "current": true, "isDefault": true, "worktreePath": null},
                {"name": "origin/main", "isRemote": true, "current": false, "isDefault": false},
                {"broken": true},
            ],
            "nextCursor": "2",
            "totalCount": 768,
            "isRepo": true,
            "hasPrimaryRemote": true,
        }));
        assert_eq!(page.refs.len(), 2);
        assert_eq!(page.total, 768);
        assert_eq!(page.status(false).as_deref(), Some("Showing 2 of 768 refs"));
        assert_eq!(page.status(true).as_deref(), Some("Loading more refs..."));
        assert_eq!(
            Page::default().status(true).as_deref(),
            Some("Loading refs...")
        );

        let whole = Page::decode(&json!({"refs": [], "nextCursor": null, "totalCount": 0}));
        assert_eq!(whole.status(false), None);
    }
}
