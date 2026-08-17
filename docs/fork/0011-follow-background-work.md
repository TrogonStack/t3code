# 0011: Follow the background work a thread left running

- PR: [TrogonStack/t3code#20](https://github.com/TrogonStack/t3code/pull/20)
- Status: active

## What you can do now

- Read what is actually running when a thread keeps working after its turn
  ends. The composer banner names the work instead of only naming a mode, so
  "Monitoring in the background" becomes the watch loops by name.
- Open Details on that banner for the full list: what each item is, its latest
  progress line, and when it last reported.
- Know what Stop does before pressing it. The banner and the list both say it
  ends every listed item at once, which is the only granularity there is.

## Why

A thread can hold background work open for hours, and the banner was the only
place that admitted it. It reported a state and offered a single destructive
button, which left two questions unanswered at the moment they matter: what is
still running, and what am I about to kill. People who could not answer either
one stopped everything to find out, which is the opposite of what the feature
is for.

Monitoring was the worse of the two states. It is by definition the state with
no live agents, so the roster people would otherwise check is empty exactly when
the banner is up, and a watch loop that is waiting on something looks identical
to one that has quietly wedged. Its progress line is the difference, and it was
already being recorded, just never shown.

## Upstream considerations

This is a presentation change over data the server already persists, with no
contract, wire, or server change, so it should go upstream as a feature. Submit
it and delete this entry once it merges.

While it is carried, the chat view is the part a sync will notice, since it
moves often upstream. The rest is additive: one shared derivation and one
self-contained popover.

Mobile is deliberately untouched: it has no background-liveness banner to
improve. Desktop wraps the web client and picks this up with it.
