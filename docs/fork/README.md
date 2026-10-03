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

- **0008** [Drop a folder on the sidebar to add a project](./0008-drop-a-folder-to-add-a-project.md)
  active, [#17](https://github.com/TrogonStack/t3code/pull/17)
- **0010** [Pull request conventions of our own](./0010-fork-pull-request-conventions.md)
  active, [#19](https://github.com/TrogonStack/t3code/pull/19)
- **0012** [The timeline scrolls only as far as its content](./0012-timeline-scrolls-only-as-far-as-its-content.md)
  active, [#21](https://github.com/TrogonStack/t3code/pull/21)
- **0015** [A logged-out Claude install reads as logged out](./0015-a-logged-out-claude-install-reads-as-logged-out.md)
  active, [#26](https://github.com/TrogonStack/t3code/pull/26)
- **0016** [Provider secrets can live in 1Password](./0016-provider-secrets-live-in-1password.md)
  active, [#27](https://github.com/TrogonStack/t3code/pull/27)
- **0017** [A revoked Claude token reads as revoked](./0017-a-revoked-claude-token-reads-as-revoked.md)
  active, [#28](https://github.com/TrogonStack/t3code/pull/28)
- **0024** [A refused merge says why, and an administrator can merge anyway](./0024-a-refused-merge-says-why.md)
  active, [#38](https://github.com/TrogonStack/t3code/pull/38)
- **0025** [A test run leaves no processes behind](./0025-a-test-run-leaves-no-processes-behind.md)
  active, [#57](https://github.com/TrogonStack/t3code/pull/57)
- **0027** [Symlinked settings stay linked](./0027-symlinked-settings-stay-linked.md)
  active, [#73](https://github.com/TrogonStack/t3code/pull/73), [#74](https://github.com/TrogonStack/t3code/pull/74)
