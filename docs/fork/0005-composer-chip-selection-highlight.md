# 0005: Selection highlight over composer chips

- PR: [TrogonStack/t3code#5](https://github.com/TrogonStack/t3code/pull/5)
- Status: active

## What you can do now

- Selecting text in the composer now visibly covers file, folder, skill, and
  terminal-context chips: each chip inside the selection gets the same
  highlight tint as the surrounding text, in the system selection color.
- The highlight tracks the selection live: partial selections cover only the
  chips inside the range, and collapsing the selection clears it.

## Why

Chips are non-editable islands, so the browser paints no selection over them;
a selection spanning several mentions was only visible in the slivers between
chips, making it impossible to tell what a copy, cut, or delete was about to
act on.

## Upstream considerations

Self-contained editor polish with no dependency on other fork divergences;
straightforward to propose upstream as is.
