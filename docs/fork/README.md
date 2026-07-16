# Fork divergence ledger

This directory tracks everything this fork carries that is not in upstream
[pingdotgg/t3code](https://github.com/pingdotgg/t3code). One numbered entry per
divergence, newest last. When upstream ships an equivalent, update the status
here and note what replaced our version instead of deleting the entry, so the
history of what we carried and why stays reconstructable.

## Statuses

- `active`: carried by this fork, not in upstream.
- `submitted`: proposed to upstream, waiting on the outcome.
- `upstreamed`: merged into upstream; our patch was dropped or reconciled.
- `superseded`: upstream shipped a different solution; our patch was dropped.

## Ledger

| #    | Divergence                                                                                            | PR                                                 | Status |
| ---- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------- | ------ |
| 0001 | [File explorer mention actions and zoom-aware context menus](./0001-file-explorer-mention-actions.md) | [#1](https://github.com/TrogonStack/t3code/pull/1) | active |
