# Fork divergence ledger

This directory tracks everything this fork carries that is not in upstream
[pingdotgg/t3code](https://github.com/pingdotgg/t3code). One numbered entry per
divergence, newest last. When upstream ships an equivalent, update the status
here and note what replaced our version instead of deleting the entry, so the
history of what we carried and why stays reconstructable.

## Writing an entry

Entries are product focused. Describe what someone can do now and why we
wanted it, not how it was built. No touched-file lists, component names, or
implementation details; the linked PR already carries all of that, and code
detail in the ledger goes stale the moment the code moves.

Each entry uses these sections:

- **What you can do now**: the user-visible capabilities, as bullets.
- **Why**: the product rationale for carrying the divergence.
- **Upstream considerations**: whether and how we would submit it upstream,
  and anything that affects the rebase burden.

## Statuses

- `active`: carried by this fork, not in upstream.
- `submitted`: proposed to upstream, waiting on the outcome.
- `upstreamed`: merged into upstream; our patch was dropped or reconciled.
- `superseded`: upstream shipped a different solution; our patch was dropped.

## Ledger

| #    | Divergence                                                                                            | PR                                                 | Status |
| ---- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------- | ------ |
| 0001 | [File explorer mention actions and zoom-aware context menus](./0001-file-explorer-mention-actions.md) | [#1](https://github.com/TrogonStack/t3code/pull/1) | active |
| 0002 | [Draft hero landing on the index route](./0002-draft-hero-landing.md)                                 | [#2](https://github.com/TrogonStack/t3code/pull/2) | active |
| 0003 | [Native subagent threads for Claude orchestrators](./0003-native-subagent-threads.md)                 | [#3](https://github.com/TrogonStack/t3code/pull/3) | active |
