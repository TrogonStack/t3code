# 0027: Symlinked settings stay linked

- PR: pending
- Status: active

## What you can do now

- Keep T3 Code's settings in a dotfiles repository and link them into the T3
  home. Changing a setting in the app writes through the link, so the
  repository copy is always the one in use.
- Edit settings from the app as often as you like without the link quietly
  turning into a standalone copy that the repository no longer sees.

## Why

Saving settings used to replace a linked file with a regular one. Nothing
failed and the app kept working, so the first sign was a later discovery that
the repository and the machine had drifted apart, with no record of which
changes happened where. Configuration that was supposed to be versioned had
stopped being versioned on the first save.

That silence is the reason to fix it rather than document it. A setup that
breaks loudly gets fixed on the spot; one that detaches without a trace costs
a reconciliation session weeks later, after the two copies have each picked up
changes the other lacks.

## Upstream considerations

A clean upstream submission. Writes still land atomically, nothing changes for
anyone who does not use links, and the behavior is what a user who links a
config file expects. Once an equivalent lands upstream, this entry goes.
