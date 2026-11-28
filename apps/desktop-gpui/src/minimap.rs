//! The transcript minimap's data, mirroring `timelineMinimapItems.ts` and the geometry helpers in
//! `MessagesTimeline.logic.ts`: one tick per user message, previewed on hover.

use crate::thread::{Entry, Kind};
use markdown::{ParseOptions, mdast::Node};
use std::sync::Arc;

pub const MIN_ITEMS: usize = 2;
const ITEM_SPACING: f32 = 8.;
/// The web's `calc(100vh - 18rem)`.
const MAX_HEIGHT_INSET: f32 = 288.;
const PERSISTENT_GUTTER: f32 = 48.;
const HIT_STRIP_LEFT: f32 = 12.;
const HIT_STRIP_MAX_WIDTH: f32 = 40.;
/// The tooltip shows three lines; bounds parsing work while long responses stream.
const PREVIEW_MARKDOWN_CHARS: usize = 1_500;
/// The transcript column the web measures the side gutter against.
pub const CONTENT_WIDTH: f32 = 720.;
const ATTACHMENT_ONLY_BOOTSTRAP_PROMPT: &str = "[User attached one or more files without additional text. Respond using the conversation context and the attached files.]";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The index of the user message in the transcript rows.
    pub row: usize,
    pub user: Arc<str>,
    pub assistant: Option<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub user: Option<String>,
    pub assistant: Option<String>,
}

fn is_attachment_only_bootstrap_prompt(text: &str) -> bool {
    let rest = text
        .get(..10)
        .filter(|head| head.eq_ignore_ascii_case("ultrathink"))
        .and_then(|_| text[10..].strip_prefix(':'))
        .map_or(text, str::trim_start);
    rest == ATTACHMENT_ONLY_BOOTSTRAP_PROMPT
}

/// One item per user message, with the turn's final assistant text.
pub fn items(entries: &[Entry]) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut in_turn = false;
    for (row, entry) in entries.iter().enumerate() {
        match entry.kind {
            Kind::User => {
                in_turn = true;
                let user = if is_attachment_only_bootstrap_prompt(&entry.text) {
                    Arc::from("")
                } else {
                    entry.text.clone()
                };
                items.push(Item {
                    row,
                    user,
                    assistant: None,
                });
            }
            Kind::Assistant if in_turn => {
                if let Some(item) = items.last_mut() {
                    item.assistant = Some(entry.text.clone());
                }
            }
            _ => {}
        }
    }
    items
}

fn compact(text: &str) -> Option<String> {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!compact.is_empty()).then_some(compact)
}

pub fn preview(item: &Item) -> Preview {
    Preview {
        user: compact(&item.user),
        assistant: item.assistant.as_deref().and_then(|text| {
            let head: String = text.chars().take(PREVIEW_MARKDOWN_CHARS).collect();
            compact(&markdown_text(&head))
        }),
    }
}

fn markdown_text(markdown: &str) -> String {
    match markdown::to_mdast(markdown, &ParseOptions::gfm()) {
        Ok(node) => node_text(&node),
        Err(_) => markdown.to_owned(),
    }
}

fn node_text(node: &Node) -> String {
    match node {
        Node::Text(n) => return n.value.clone(),
        Node::InlineCode(n) => return n.value.clone(),
        Node::Code(n) => return n.value.clone(),
        Node::InlineMath(n) => return n.value.clone(),
        Node::Math(n) => return n.value.clone(),
        Node::Html(n) => return html_text(&n.value),
        Node::Break(_) => return " ".to_owned(),
        Node::Image(n) => return n.alt.clone(),
        Node::ImageReference(n) => return n.alt.clone(),
        _ => {}
    }
    let Some(children) = node.children() else {
        return String::new();
    };
    let inline = matches!(
        node,
        Node::Paragraph(_)
            | Node::Heading(_)
            | Node::TableCell(_)
            | Node::Emphasis(_)
            | Node::Strong(_)
            | Node::Delete(_)
            | Node::Link(_)
            | Node::LinkReference(_)
    );
    let parts: Vec<String> = children.iter().map(node_text).collect();
    let text = parts.join(if inline { "" } else { " " });
    // A thread autolink's only text is its URL.
    if matches!(node, Node::Link(_)) && is_thread_link(&text) {
        return "a thread".to_owned();
    }
    text
}

fn is_thread_link(text: &str) -> bool {
    ["t3-thread://v1/", "t3-thread://v2/"].iter().any(|prefix| {
        text.strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty())
    })
}

const BLOCK_TAGS: [&str; 34] = [
    "address",
    "article",
    "aside",
    "blockquote",
    "br",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "th",
    "thead",
];

/// Drops comments and tags; a block-level tag leaves a space behind.
fn html_text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
        } else if let Some((len, name)) = tag_at(rest) {
            if BLOCK_TAGS.contains(&name.to_ascii_lowercase().as_str())
                || ["tr", "ul"].contains(&name.to_ascii_lowercase().as_str())
            {
                out.push(' ');
            }
            rest = &rest[len..];
        } else {
            out.push('<');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// The length and name of the tag at the start of `text`, like `</?([a-z][\w-]*)(...)*>`.
fn tag_at(text: &str) -> Option<(usize, &str)> {
    let body = text.strip_prefix('<')?;
    let body = body.strip_prefix('/').unwrap_or(body);
    let start = text.len() - body.len();
    if !body.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let name_len = body
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(body.len());
    let mut chars = body[name_len..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '>' => return Some((start + name_len + i + 1, &body[..name_len])),
            '<' => return None,
            '"' | '\'' => {
                chars.find(|&(_, q)| q == c)?;
            }
            _ => {}
        }
    }
    None
}

/// Where item `index` sits along the rail, 0 to 100.
pub fn top_percent(index: usize, count: usize) -> f32 {
    if count <= 1 {
        return 0.;
    }
    index.min(count - 1) as f32 / (count - 1) as f32 * 100.
}

pub fn index_from_pointer(
    count: usize,
    rail_top: f32,
    rail_height: f32,
    pointer_y: f32,
) -> Option<usize> {
    if count == 0 || rail_height <= 0. {
        return None;
    }
    if count == 1 {
        return Some(0);
    }
    let progress = ((pointer_y - rail_top) / rail_height).clamp(0., 1.);
    Some(((progress * (count - 1) as f32).round() as usize).min(count - 1))
}

pub fn rail_height(count: usize, viewport_height: f32) -> f32 {
    let natural = (count.saturating_sub(1) as f32 * ITEM_SPACING).max(1.);
    natural.min((viewport_height - MAX_HEIGHT_INSET).max(0.))
}

fn side_gutter(viewport_width: f32) -> f32 {
    if viewport_width <= 0. {
        return 0.;
    }
    (viewport_width - viewport_width.min(CONTENT_WIDTH)) / 2.
}

pub fn has_persistent_gutter(viewport_width: f32) -> bool {
    side_gutter(viewport_width) >= PERSISTENT_GUTTER
}

pub fn hit_strip_width(viewport_width: f32) -> f32 {
    (side_gutter(viewport_width).floor() - HIT_STRIP_LEFT).clamp(0., HIT_STRIP_MAX_WIDTH)
}

pub const HIT_STRIP_LEFT_PX: f32 = HIT_STRIP_LEFT;

/// The strip's length factor while hovering: a fisheye around the hovered tick.
pub fn strip_scale(distance: Option<usize>) -> f32 {
    match distance {
        Some(0) => 1.,
        Some(1) => 0.667,
        Some(2) => 0.417,
        _ => 0.333,
    }
}

/// How far the card shifts along its tick, as a fraction of its height: the ends stay in the rail.
pub fn card_shift(index: usize, count: usize) -> f32 {
    if index == 0 {
        0.
    } else if index + 1 == count {
        -1.
    } else {
        -0.5
    }
}

/// The items on screen, given the first and last visible rows.
pub fn in_view(row: usize, visible: Option<(usize, usize)>) -> bool {
    visible.is_some_and(|(start, end)| (start..=end).contains(&row))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: Kind, text: &str) -> Entry {
        Entry {
            id: text.to_owned(),
            kind,
            text: text.into(),
        }
    }

    fn assistant_preview(text: &str) -> Option<String> {
        preview(&Item {
            row: 0,
            user: "".into(),
            assistant: Some(text.into()),
        })
        .assistant
    }

    #[test]
    fn bootstrap_text_is_omitted_from_the_label_and_card() {
        for prefix in ["", "Ultrathink:\n"] {
            let text = format!("{prefix}{ATTACHMENT_ONLY_BOOTSTRAP_PROMPT}");
            let found = items(&[entry(Kind::User, &text)]);
            assert_eq!(found.len(), 1);
            assert_eq!(&*found[0].user, "");
            assert_eq!(found[0].assistant, None);
            assert_eq!(preview(&found[0]).user, None);
        }
    }

    #[test]
    fn items_pair_each_user_message_with_the_turns_final_assistant_text() {
        let found = items(&[
            entry(Kind::Assistant, "orphan"),
            entry(Kind::User, "one"),
            entry(Kind::Assistant, "draft"),
            entry(Kind::Work, "$ ls"),
            entry(Kind::Assistant, "final"),
            entry(Kind::User, "two"),
        ]);
        let rows: Vec<_> = found
            .iter()
            .map(|item| (item.row, &*item.user, item.assistant.as_deref()))
            .collect();
        assert_eq!(rows, vec![(1, "one", Some("final")), (5, "two", None)]);
    }

    #[test]
    fn previews_compact_whitespace() {
        let item = Item {
            row: 0,
            user: "  fix\n\nthe   bug ".into(),
            assistant: None,
        };
        assert_eq!(preview(&item).user.as_deref(), Some("fix the bug"));
    }

    #[test]
    fn a_thread_autolink_previews_as_prose_not_its_url() {
        assert_eq!(
            assistant_preview(
                "Started <t3-thread://v2/env-1/thread-1> and [Fix CI](t3-thread://v2/env-1/t2)."
            )
            .as_deref(),
            Some("Started a thread and Fix CI.")
        );
    }

    #[test]
    fn markdown_previews_as_plain_text() {
        assert_eq!(
            assistant_preview("# Title\n\nSome **bold** and `code`.\n\n- one\n- two").as_deref(),
            Some("Title Some bold and code. one two")
        );
        assert_eq!(
            assistant_preview("a<br>b <!-- hidden --><span>c</span>").as_deref(),
            Some("a b c")
        );
    }

    #[test]
    fn only_the_head_of_a_long_reply_is_parsed() {
        let long = "x".repeat(PREVIEW_MARKDOWN_CHARS + 500);
        assert_eq!(
            assistant_preview(&long).map(|text| text.len()),
            Some(PREVIEW_MARKDOWN_CHARS)
        );
    }

    #[test]
    fn rail_geometry_follows_the_web() {
        assert_eq!(top_percent(0, 5), 0.);
        assert_eq!(top_percent(2, 5), 50.);
        assert_eq!(top_percent(9, 5), 100.);
        assert_eq!(index_from_pointer(5, 100., 32., 100. + 16.), Some(2));
        assert_eq!(index_from_pointer(5, 100., 32., 500.), Some(4));
        assert_eq!(index_from_pointer(0, 100., 32., 110.), None);
        assert_eq!(rail_height(5, 2000.), 32.);
        assert_eq!(rail_height(1000, 600.), 312.);
        assert!(has_persistent_gutter(900.));
        assert!(!has_persistent_gutter(800.));
        assert_eq!(hit_strip_width(900.), 40.);
        assert_eq!(hit_strip_width(750.), 3.);
        assert_eq!(hit_strip_width(720.), 0.);
        assert_eq!(card_shift(0, 4), 0.);
        assert_eq!(card_shift(3, 4), -1.);
        assert_eq!(card_shift(1, 4), -0.5);
        assert!(in_view(3, Some((2, 4))) && !in_view(5, Some((2, 4))) && !in_view(3, None));
    }
}
