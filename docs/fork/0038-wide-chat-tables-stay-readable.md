# 0038: Wide tables in a reply stay readable

- PR: [TrogonStack/t3code#236](https://github.com/TrogonStack/t3code/pull/236)
- Status: active

## What you can do now

- Read a wide table in an agent's reply without its cells wrapping into tall, narrow columns.
  Columns keep a sensible minimum width, and a table wider than the reading column grows into
  the free pane width or scrolls sideways with its first column kept in view.
- Open a table that still scrolls in a near full-screen view with a sticky header row and first
  column, and copy it from there.

## Why

Agents often answer with comparison tables. Squeezed into the reading column, a table of six or
seven columns became unreadable and many screens tall, while prose next to it was fine.

## Upstream considerations

A reasonable candidate to submit. The rebase burden sits in the chat markdown table and its
copy menu, which upstream has been changing (most recently for translations), so a sync touching
either needs to carry the width floor and the expanded view forward.
