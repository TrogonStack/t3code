//! The diff surface's data: the `review.getDiffPreview` result (`ReviewDiffPreviewResult` in
//! `packages/contracts/src/review.ts`) and the unified patch it carries, parsed into the files,
//! hunks and lines `DiffPanel.tsx` shows. The review RPCs are not in the generated wire types,
//! so the reply is read from JSON like `strip::Branch`.

use serde_json::Value;
use std::cmp::Ordering;

/// The web's `formatCompactDiffCount` (`DiffStatLabel.tsx`).
pub fn format_count(value: u64) -> String {
    let compact = |value: f64, unit: &str| {
        let text = if value < 10. {
            format!("{value:.1}").trim_end_matches(".0").to_owned()
        } else {
            format!("{}", value.round())
        };
        format!("{text}{unit}")
    };
    if value < 1_000 {
        value.to_string()
    } else if value < 999_500 {
        compact(value as f64 / 1_000., "k")
    } else if value < 999_500_000 {
        compact(value as f64 / 1_000_000., "m")
    } else {
        compact(value as f64 / 1_000_000_000., "b")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Modified,
    Added,
    Deleted,
    Renamed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Context,
    Added,
    Removed,
    /// `\ No newline at end of file`.
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: Kind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub previous_path: Option<String>,
    pub change: Change,
    pub binary: bool,
    pub additions: u64,
    pub deletions: u64,
    pub hunks: Vec<Hunk>,
}

/// One diff source: the working tree against `HEAD`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preview {
    pub files: Vec<File>,
    /// The server cut the patch short, so some files have no hunks; counts still cover them.
    pub truncated: bool,
}

impl Preview {
    pub fn totals(&self) -> (u64, u64) {
        self.files
            .iter()
            .fold((0, 0), |(additions, deletions), file| {
                (additions + file.additions, deletions + file.deletions)
            })
    }

    /// Reads a `ReviewDiffPreviewResult`, using its first source. The server's per-file stats
    /// win over counted lines, since a truncated patch undercounts.
    pub fn decode(value: &Value) -> Option<Self> {
        let source = value.get("sources")?.as_array()?.first()?;
        let diff = source.get("diff").and_then(Value::as_str).unwrap_or("");
        let mut files = parse(diff);
        let flag = |holder: &Value, key| holder.get(key).and_then(Value::as_bool) == Some(true);
        let truncated = flag(source, "truncated")
            || flag(source, "patchesDeferred")
            || flag(value, "incomplete");
        for stat in source
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(path) = stat.get("path").and_then(Value::as_str) else {
                continue;
            };
            let count = |key| stat.get(key).and_then(Value::as_u64).unwrap_or(0);
            let (additions, deletions) = (count("additions"), count("deletions"));
            match files.iter_mut().find(|file| file.path == path) {
                Some(file) => (file.additions, file.deletions) = (additions, deletions),
                None => files.push(File {
                    path: path.to_owned(),
                    previous_path: stat
                        .get("previousPath")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    change: Change::Modified,
                    binary: false,
                    additions,
                    deletions,
                    hunks: Vec::new(),
                }),
            }
        }
        files.sort_by(|a, b| natural_cmp(&a.path, &b.path));
        Some(Self { files, truncated })
    }
}

/// Paths in file order, digits compared as numbers, like the web's `Intl.Collator` with `numeric`.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn number(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        let mut digits = String::new();
        while let Some(digit) = chars.next_if(char::is_ascii_digit) {
            digits.push(digit);
        }
        digits.trim_start_matches('0').to_owned()
    }
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (left, right) = (number(&mut a), number(&mut b));
                let order = left.len().cmp(&right.len()).then_with(|| left.cmp(&right));
                if order.is_ne() {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order.is_ne() {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Splits a unified diff (`git diff` output) into files, hunks and numbered lines.
pub fn parse(patch: &str) -> Vec<File> {
    let mut files: Vec<File> = Vec::new();
    let mut counters = (0u32, 0u32);
    let mut in_hunk = false;
    for raw in patch.lines() {
        if let Some(rest) = raw.strip_prefix("diff --git ") {
            let (old, new) = split_git_header(rest);
            files.push(File {
                path: new,
                previous_path: Some(old),
                change: Change::Modified,
                binary: false,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
            in_hunk = false;
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if !in_hunk {
            if raw.starts_with("new file mode") {
                file.change = Change::Added;
            } else if raw.starts_with("deleted file mode") {
                file.change = Change::Deleted;
            } else if let Some(path) = raw.strip_prefix("rename from ") {
                file.change = Change::Renamed;
                file.previous_path = Some(path.to_owned());
            } else if let Some(path) = raw.strip_prefix("rename to ") {
                file.path = path.to_owned();
            } else if raw.starts_with("Binary files ") || raw.starts_with("GIT binary patch") {
                file.binary = true;
            } else if let Some(path) = raw.strip_prefix("--- ") {
                if let Some(path) = side_path(path, "a/") {
                    file.previous_path = Some(path);
                }
            } else if let Some(path) = raw.strip_prefix("+++ ").and_then(|p| side_path(p, "b/")) {
                file.path = path;
            }
        }
        if let Some(header) = raw.strip_prefix("@@ ") {
            let Some((old, new)) = hunk_starts(header) else {
                continue;
            };
            counters = (old, new);
            in_hunk = true;
            file.hunks.push(Hunk {
                header: raw.to_owned(),
                lines: Vec::new(),
            });
            continue;
        }
        if !in_hunk {
            continue;
        }
        let Some(hunk) = file.hunks.last_mut() else {
            continue;
        };
        let (kind, text) = match raw.as_bytes().first() {
            Some(b'+') => (Kind::Added, &raw[1..]),
            Some(b'-') => (Kind::Removed, &raw[1..]),
            Some(b'\\') => (Kind::Note, raw),
            Some(b' ') => (Kind::Context, &raw[1..]),
            None => (Kind::Context, raw),
            Some(_) => continue,
        };
        let (old, new) = match kind {
            Kind::Added => {
                file.additions += 1;
                counters.1 += 1;
                (None, Some(counters.1 - 1))
            }
            Kind::Removed => {
                file.deletions += 1;
                counters.0 += 1;
                (Some(counters.0 - 1), None)
            }
            Kind::Context => {
                counters.0 += 1;
                counters.1 += 1;
                (Some(counters.0 - 1), Some(counters.1 - 1))
            }
            Kind::Note => (None, None),
        };
        hunk.lines.push(Line {
            kind,
            old,
            new,
            text: text.to_owned(),
        });
    }
    for file in &mut files {
        if file.previous_path.as_deref() == Some(file.path.as_str()) {
            file.previous_path = None;
        }
    }
    files
}

/// `a/old b/new` from a `diff --git` line; with spaces in names the split is a guess, which the
/// `---` and `+++` lines correct when the file has hunks.
fn split_git_header(rest: &str) -> (String, String) {
    let strip = |text: &str| {
        let text = text.trim_matches('"');
        let text = text
            .strip_prefix("a/")
            .or(text.strip_prefix("b/"))
            .unwrap_or(text);
        text.to_owned()
    };
    if let Some(index) = rest.find(" b/") {
        let (old, new) = rest.split_at(index);
        return (strip(old), strip(new.trim_start()));
    }
    let (old, new) = rest.split_once(' ').unwrap_or((rest, rest));
    (strip(old), strip(new))
}

/// A `---` or `+++` path without its `a/` or `b/` prefix; `/dev/null` is no path.
fn side_path(text: &str, prefix: &str) -> Option<String> {
    let text = text.split('\t').next().unwrap_or(text).trim_matches('"');
    if text == "/dev/null" {
        return None;
    }
    Some(text.strip_prefix(prefix).unwrap_or(text).to_owned())
}

/// The old and new start lines of `-a,b +c,d @@ ...`.
fn hunk_starts(header: &str) -> Option<(u32, u32)> {
    let ranges = header.split(" @@").next()?;
    let (old, new) = ranges.split_once(' ')?;
    let start = |range: &str, sign: char| {
        range
            .strip_prefix(sign)?
            .split(',')
            .next()?
            .parse::<u32>()
            .ok()
    };
    Some((start(old, '-')?, start(new, '+')?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PATCH: &str = "diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,3 +1,3 @@ fn main
 keep
-old
+new
 tail
@@ -10,2 +10,3 @@
 ctx
+added
 more
\\ No newline at end of file
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+one
+two
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
diff --git a/before.rs b/after.rs
similarity index 90%
rename from before.rs
rename to after.rs
";

    #[test]
    fn a_patch_splits_into_files_with_counts_and_kinds() {
        let files = parse(PATCH);
        let summary: Vec<_> = files
            .iter()
            .map(|file| {
                (
                    file.path.as_str(),
                    file.change,
                    file.additions,
                    file.deletions,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("src/a.rs", Change::Modified, 2, 1),
                ("new.txt", Change::Added, 2, 0),
                ("gone.txt", Change::Deleted, 0, 1),
                ("img.png", Change::Modified, 0, 0),
                ("after.rs", Change::Renamed, 0, 0),
            ]
        );
        assert!(files[3].binary);
        assert_eq!(files[4].previous_path.as_deref(), Some("before.rs"));
        assert_eq!(files[0].previous_path, None);
    }

    #[test]
    fn lines_carry_their_numbers_per_side() {
        let files = parse(PATCH);
        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.header, "@@ -1,3 +1,3 @@ fn main");
        let numbers: Vec<_> = hunk
            .lines
            .iter()
            .map(|line| (line.kind, line.old, line.new, line.text.as_str()))
            .collect();
        assert_eq!(
            numbers,
            [
                (Kind::Context, Some(1), Some(1), "keep"),
                (Kind::Removed, Some(2), None, "old"),
                (Kind::Added, None, Some(2), "new"),
                (Kind::Context, Some(3), Some(3), "tail"),
            ]
        );
        let second = &files[0].hunks[1];
        assert_eq!(second.lines[1].new, Some(11));
        assert_eq!(second.lines[3].kind, Kind::Note);
    }

    #[test]
    fn the_servers_stats_win_and_list_files_the_patch_lacks() {
        let value = json!({"sources": [{
            "diff": PATCH,
            "truncated": true,
            "files": [
                {"path": "src/a.rs", "previousPath": null, "additions": 40, "deletions": 3},
                {"path": "big.lock", "previousPath": null, "additions": 9000, "deletions": 0},
            ],
        }]});
        let preview = Preview::decode(&value).expect("preview");
        assert!(preview.truncated);
        let big = preview
            .files
            .iter()
            .find(|f| f.path == "big.lock")
            .expect("big");
        assert!(big.hunks.is_empty());
        let a = preview
            .files
            .iter()
            .find(|f| f.path == "src/a.rs")
            .expect("a");
        assert_eq!((a.additions, a.deletions), (40, 3));
        assert_eq!(preview.totals(), (9042, 4));
        assert!(Preview::decode(&json!({"sources": []})).is_none());
    }

    #[test]
    fn files_sort_with_numbers_compared_as_numbers() {
        let mut names = ["b10.rs", "b2.rs", "A.rs"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["A.rs", "b2.rs", "b10.rs"]);
    }

    #[test]
    fn counts_compact_like_the_web() {
        let shown: Vec<_> = [0, 999, 1000, 1300, 12_400, 999_499, 1_500_000]
            .into_iter()
            .map(format_count)
            .collect();
        assert_eq!(shown, ["0", "999", "1k", "1.3k", "12k", "999k", "1.5m"]);
    }
}
