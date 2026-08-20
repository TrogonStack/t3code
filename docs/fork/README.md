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

- **0003** [Native subagent threads for Claude orchestrators](./0003-native-subagent-threads.md)
  active, [#3](https://github.com/TrogonStack/t3code/pull/3)
- **0006** [Fork schema on its own migration ledger](./0006-fork-migration-ledger.md)
  active, [#13](https://github.com/TrogonStack/t3code/pull/13), [#16](https://github.com/TrogonStack/t3code/pull/16)
- **0007** [API-key Codex installs are not reported as broken](./0007-codex-api-key-auth-is-supported.md)
  active, [#15](https://github.com/TrogonStack/t3code/pull/15)
- **0008** [Drop a folder on the sidebar to add a project](./0008-drop-a-folder-to-add-a-project.md)
  active, [#17](https://github.com/TrogonStack/t3code/pull/17)
- **0009** [Grok reports the account it is signed in as](./0009-grok-reports-its-authenticated-account.md)
  active, [#18](https://github.com/TrogonStack/t3code/pull/18)
- **0010** [Pull request conventions of our own](./0010-fork-pull-request-conventions.md)
  active, [#19](https://github.com/TrogonStack/t3code/pull/19)
- **0011** [Follow the background work a thread left running](./0011-follow-background-work.md)
  active, [#20](https://github.com/TrogonStack/t3code/pull/20)
- **0012** [The timeline scrolls only as far as its content](./0012-timeline-scrolls-only-as-far-as-its-content.md)
  active, [#21](https://github.com/TrogonStack/t3code/pull/21)
- **0013** [Keep your place in a long review](./0013-keep-your-place-in-a-review.md)
  active, [#23](https://github.com/TrogonStack/t3code/pull/23)
- **0015** [A logged-out Claude install reads as logged out](./0015-a-logged-out-claude-install-reads-as-logged-out.md)
  active, [#26](https://github.com/TrogonStack/t3code/pull/26)
- **0016** [Provider secrets can live in 1Password](./0016-provider-secrets-live-in-1password.md)
  active
