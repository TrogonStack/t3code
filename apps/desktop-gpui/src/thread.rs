//! The transcript read model built from `orchestration.subscribeThread`, mirroring
//! `packages/client-runtime/src/state/orchestrationV2Projection.ts` for what this client shows:
//! messages and a one-line summary of tool work. The success type is opaque in the wire schema,
//! so items are read straight from the JSON and anything unreadable is skipped.

use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    User,
    Assistant,
    /// A command, file change or search, shown as one muted line.
    Work,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub text: Arc<str>,
}

struct Row {
    entry: Entry,
    ordinal: i64,
    /// Inherited rows (from a forked thread) keep their place; only local rows are reordered.
    local: bool,
}

#[derive(Default)]
pub struct ThreadState {
    rows: Vec<Row>,
    sequence: i64,
    seeded: bool,
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// The row an item shows as; `None` for the item types this client does not draw.
fn entry_of(item: &Value) -> Option<Entry> {
    let text = |key: &str| item.get(key).and_then(Value::as_str);
    let (kind, text) = match item.get("type")?.as_str()? {
        "user_message" => (Kind::User, text("text")?.to_owned()),
        "assistant_message" => (Kind::Assistant, text("text")?.to_owned()),
        "command_execution" => (Kind::Work, format!("$ {}", first_line(text("input")?))),
        "file_change" => (Kind::Work, format!("Edited {}", text("fileName")?)),
        "file_search" => (Kind::Work, "Searched files".to_owned()),
        "web_search" => (Kind::Work, "Searched the web".to_owned()),
        _ => return None,
    };
    Some(Entry {
        id: item.get("id")?.as_str()?.to_owned(),
        kind,
        text: text.into(),
    })
}

fn row_of(item: &Value, local: bool) -> Option<Row> {
    Some(Row {
        entry: entry_of(item)?,
        ordinal: item.get("ordinal")?.as_i64()?,
        local,
    })
}

impl ThreadState {
    /// False until the first snapshot arrives.
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// The transcript in order. Messages that have not received text yet are left out.
    pub fn entries(&self) -> Vec<Entry> {
        self.rows
            .iter()
            .map(|row| &row.entry)
            .filter(|entry| !entry.text.is_empty())
            .cloned()
            .collect()
    }

    /// Applies one stream item. Returns whether the transcript may have changed.
    pub fn apply(&mut self, item: &Value) -> bool {
        match item.get("kind").and_then(Value::as_str) {
            Some("snapshot") => {
                let Some(sequence) = item.get("snapshotSequence").and_then(Value::as_i64) else {
                    return false;
                };
                let rows = item.pointer("/projection/visibleTurnItems");
                self.rows = rows
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|row| {
                        let local = row.get("visibility").and_then(Value::as_str) == Some("local");
                        row_of(row.get("item")?, local)
                    })
                    .collect();
                self.sequence = sequence;
                self.seeded = true;
                true
            }
            Some("event") => {
                let Some(sequence) = item.get("sequence").and_then(Value::as_i64) else {
                    return false;
                };
                if !self.advance(sequence) {
                    return false;
                }
                let event = &item["event"];
                // Streaming text arrives as whole-item updates, not deltas.
                if event.get("type").and_then(Value::as_str) != Some("turn-item.updated") {
                    return false;
                }
                event
                    .get("payload")
                    .and_then(|payload| row_of(payload, true))
                    .is_some_and(|row| self.upsert(row))
            }
            Some("unknown-event") => {
                if let Some(sequence) = item.get("sequence").and_then(Value::as_i64) {
                    self.advance(sequence);
                }
                false
            }
            _ => false,
        }
    }

    /// Updates land on the newest rows, so the search starts there. A moved item goes before the
    /// first later local row, like `upsertVisibleTurnItem`.
    fn upsert(&mut self, row: Row) -> bool {
        let found = self.rows.iter().rposition(|r| r.entry.id == row.entry.id);
        if let Some(index) = found {
            if self.rows[index].ordinal == row.ordinal {
                let changed = self.rows[index].entry != row.entry;
                self.rows[index].entry = row.entry;
                return changed;
            }
            self.rows.remove(index);
        }
        let at = self
            .rows
            .iter()
            .position(|r| r.local && (r.ordinal, &r.entry.id) > (row.ordinal, &row.entry.id))
            .unwrap_or(self.rows.len());
        self.rows.insert(at, row);
        true
    }

    /// Events at or below the last applied sequence are stale.
    fn advance(&mut self, sequence: i64) -> bool {
        if sequence <= self.sequence {
            return false;
        }
        self.sequence = sequence;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: &str, ordinal: i64, kind: &str, text: &str) -> Value {
        match kind {
            "command_execution" => {
                json!({"id": id, "ordinal": ordinal, "type": kind, "input": text})
            }
            _ => json!({"id": id, "ordinal": ordinal, "type": kind, "text": text}),
        }
    }

    fn snapshot(sequence: i64, items: Vec<(&str, Value)>) -> Value {
        let rows: Vec<Value> = items
            .into_iter()
            .map(|(visibility, item)| json!({"visibility": visibility, "item": item}))
            .collect();
        json!({
            "kind": "snapshot",
            "snapshotSequence": sequence,
            "projection": {"visibleTurnItems": rows},
        })
    }

    fn updated(sequence: i64, payload: Value) -> Value {
        json!({
            "kind": "event",
            "sequence": sequence,
            "event": {"type": "turn-item.updated", "payload": payload},
        })
    }

    fn texts(state: &ThreadState) -> Vec<String> {
        state.entries().iter().map(|e| e.text.to_string()).collect()
    }

    fn seeded() -> ThreadState {
        let mut state = ThreadState::default();
        assert!(!state.is_seeded());
        state.apply(&snapshot(
            10,
            vec![
                ("local", item("u1", 1, "user_message", "hi")),
                ("local", item("a1", 2, "assistant_message", "hello")),
                ("local", item("r1", 3, "reasoning", "thinking")),
            ],
        ));
        state
    }

    #[test]
    fn snapshot_seeds_messages_and_skips_items_it_does_not_draw() {
        let state = seeded();
        assert!(state.is_seeded());
        let kinds: Vec<_> = state.entries().iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec![Kind::User, Kind::Assistant]);
        assert_eq!(texts(&state), vec!["hi", "hello"]);
    }

    #[test]
    fn streaming_updates_replace_the_item_text_in_place() {
        let mut state = seeded();
        assert!(state.apply(&updated(
            11,
            item("a1", 2, "assistant_message", "hello wor")
        )));
        assert!(state.apply(&updated(
            12,
            item("a1", 2, "assistant_message", "hello world")
        )));
        assert_eq!(texts(&state), vec!["hi", "hello world"]);
        // The same text again changes nothing.
        assert!(!state.apply(&updated(
            13,
            item("a1", 2, "assistant_message", "hello world")
        )));
    }

    #[test]
    fn new_items_land_by_ordinal_and_work_shows_as_a_line() {
        let mut state = seeded();
        assert!(state.apply(&updated(11, item("a2", 5, "assistant_message", "done"))));
        assert!(state.apply(&updated(
            12,
            item("c1", 4, "command_execution", "ls -la\nmore")
        )));
        assert_eq!(texts(&state), vec!["hi", "hello", "$ ls -la", "done"]);
        assert_eq!(state.entries()[2].kind, Kind::Work);
    }

    #[test]
    fn an_empty_streaming_message_stays_hidden_until_it_has_text() {
        let mut state = seeded();
        assert!(state.apply(&updated(11, item("a2", 5, "assistant_message", ""))));
        assert_eq!(texts(&state), vec!["hi", "hello"]);
        state.apply(&updated(12, item("a2", 5, "assistant_message", "x")));
        assert_eq!(texts(&state), vec!["hi", "hello", "x"]);
    }

    #[test]
    fn events_at_or_below_the_last_sequence_are_ignored() {
        let mut state = seeded();
        assert!(!state.apply(&updated(10, item("a1", 2, "assistant_message", "stale"))));
        assert!(!state.apply(&updated(3, item("a9", 9, "assistant_message", "old"))));
        assert_eq!(texts(&state), vec!["hi", "hello"]);
        assert!(state.apply(&updated(11, item("a1", 2, "assistant_message", "fresh"))));
        assert!(!state.apply(&updated(11, item("a1", 2, "assistant_message", "again"))));
        assert_eq!(texts(&state), vec!["hi", "fresh"]);
    }

    #[test]
    fn unknown_events_advance_the_cursor_and_other_events_change_nothing() {
        let mut state = seeded();
        assert!(!state.apply(&json!({"kind": "unknown-event", "sequence": 11, "eventType": "x"})));
        assert!(!state.apply(&updated(11, item("a1", 2, "assistant_message", "late"))));
        let message = json!({"kind": "event", "sequence": 12, "event": {"type": "run.updated"}});
        assert!(!state.apply(&message));
        assert!(!state.apply(&json!({"kind": "synchronized"})));
        assert_eq!(texts(&state), vec!["hi", "hello"]);
    }

    #[test]
    fn a_later_snapshot_replaces_everything() {
        let mut state = seeded();
        state.apply(&snapshot(
            4,
            vec![("local", item("u2", 1, "user_message", "again"))],
        ));
        assert_eq!(texts(&state), vec!["again"]);
        assert!(state.apply(&updated(5, item("a3", 2, "assistant_message", "ok"))));
        assert_eq!(texts(&state), vec!["again", "ok"]);
    }

    #[test]
    fn inherited_rows_keep_their_place_when_a_local_item_is_inserted() {
        let mut state = ThreadState::default();
        state.apply(&snapshot(
            1,
            vec![
                ("inherited", item("u1", 9, "user_message", "parent")),
                ("local", item("u2", 3, "user_message", "child")),
            ],
        ));
        state.apply(&updated(2, item("a1", 2, "assistant_message", "first")));
        assert_eq!(texts(&state), vec!["parent", "first", "child"]);
    }
}
