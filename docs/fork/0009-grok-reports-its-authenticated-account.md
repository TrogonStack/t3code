# 0009: Grok reports the account it is signed in as

- PR: [TrogonStack/t3code#18](https://github.com/TrogonStack/t3code/pull/18)
- Status: active

## What you can do now

- See which account a working Grok install is signed in as in Settings,
  blurred until you click it, the same way Codex and Claude already report
  theirs. A ready Grok provider no longer says its authentication could not be
  verified.
- Tell a signed-out Grok CLI apart from a broken one. Missing credentials read
  as not authenticated with a prompt to sign in, instead of looking like any
  other startup failure.
- Pick from the models your account actually has, since a working sign-in is
  what the model list comes from.

## Why

Settings exists to answer one question: is this provider working. A provider
that reported itself ready and unverifiable in the same breath answered that
question with a shrug, and the only way to find out was to start a thread and
see whether it failed. Grok tells us who is signed in every time T3 Code
starts it up, so this was information we already had and did not show.

The signed-out case matters just as much. Anyone who has not signed in needs
to be told to sign in, not handed a generic failure that reads like a bug in
T3 Code.

## Upstream considerations

Nothing here is fork-specific, so this belongs upstream as an ordinary bug
fix. Submit it, then delete this entry once it merges. It touches the shared
Grok provider and the shared ACP session runtime, so a sync must not drop it.
The runtime change is additive, which keeps the rebase burden small, but it is
the piece most likely to move under us upstream.
