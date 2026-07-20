# Fork divergence ledger

This directory tracks only what this fork currently carries that is not in
upstream [pingdotgg/t3code](https://github.com/pingdotgg/t3code). One numbered
entry per divergence, newest last. Once upstream merges an equivalent, the
divergence is gone, so its entry is deleted outright; git history is the
record if it is ever needed. Numbers are never reused.

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

## Ledger

| #    | Divergence                                                                            | PR                                                 | Status |
| ---- | ------------------------------------------------------------------------------------- | -------------------------------------------------- | ------ |
| 0003 | [Native subagent threads for Claude orchestrators](./0003-native-subagent-threads.md) | [#3](https://github.com/TrogonStack/t3code/pull/3) | active |
