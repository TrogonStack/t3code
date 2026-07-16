# 0001: File explorer mention actions and zoom-aware context menus

- PR: [TrogonStack/t3code#1](https://github.com/TrogonStack/t3code/pull/1)
- Status: active

## What you can do now

- Right-click a file or directory in the Files panel and pick **Add to chat**
  to drop it into the active composer as a file pill, or **Copy mention** to
  put it on the clipboard and paste it wherever it belongs in a prompt.
- Pasting mention text into the composer renders the file pill. This works
  for any pasted mention, including ones copied out of an earlier message,
  not just ones coming from the file explorer.
- Context menus across the desktop app open at the cursor even when the
  window is zoomed. Before this, any zoom level made menus drift away from
  the click, worst near the right edge of the window.

## Why

Attaching files to a prompt required typing @-mentions by hand even though
the file explorer already has the file in front of you. Browsing and
prompting should be one flow.

## Upstream considerations

The zoomed context menu drift is a genuine upstream bug and that part is the
strongest candidate to submit once upstream opens contributions. The mention
actions are a feature upstream may or may not want; they depend only on
public composer and tree APIs, so the rebase burden is low.
