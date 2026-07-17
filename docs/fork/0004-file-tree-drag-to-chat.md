# 0004: Drag files from the explorer into the chat

- PR: [TrogonStack/t3code#4](https://github.com/TrogonStack/t3code/pull/4)
- Status: active

## What you can do now

- Drag any file or folder from the workspace file explorer and drop it on the
  chat composer; it lands as the same mention pill that "@" autocomplete and
  "Add to chat" produce, ready to send. Dragging a multi-selection (shift or
  cmd click) drops every selected entry as its own pill.
- Dragging never acts like a click: the dragged rows are not opened and are
  not left selected, so the explorer behaves the same after a drag as before
  it.
- While a tree drag hovers over the composer, the composer highlights the
  same way it does for image drops, so the drop target is obvious.
- Rearranging files inside the explorer via drag stays disabled; dragging is
  only a way to hand a path to the conversation.

## Why

Pointing the agent at a file is the most common way to scope a request, and
the pointer is usually already on the file in the explorer. Right-click plus
"Add to chat" works but is two steps and a menu; dragging the file onto the
composer is the gesture people already try first coming from editors like
VS Code and Cursor.

## Upstream considerations

Builds on the mention actions from divergence 0001, so it inherits that
dependency if proposed upstream. The composer already had a drag pipeline for
image attachments; this adds a parallel path for tree drags without touching
the existing one, keeping the rebase surface small.
