# 0043: Escape returns focus to the composer

- PR: [TrogonStack/t3code#169](https://github.com/TrogonStack/t3code/pull/169)
- Status: active

## What you can do now

- Press Escape to get back to typing after clicking into the timeline or dismissing a popup, when
  nothing else on the screen wants the key.

## Why

In that state focus sits on nothing and returning to the composer takes a mouse trip. Escape
already did nothing there, so claiming it costs no existing behavior.

## Upstream considerations

A reasonable candidate to submit. The rebase burden sits in the chat shortcut handler, where
upstream keeps adding Escape handling for find, side chats, and popovers, so a sync touching it
needs to keep the composer fallback last.
