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

- **0010** [Pull request conventions of our own](./0010-fork-pull-request-conventions.md)
  active, [#19](https://github.com/TrogonStack/t3code/pull/19)
- **0017** [A revoked Claude token reads as revoked](./0017-a-revoked-claude-token-reads-as-revoked.md)
  active, [#28](https://github.com/TrogonStack/t3code/pull/28)
- **0024** [A refused merge says why, and an administrator can merge anyway](./0024-a-refused-merge-says-why.md)
  active, [#38](https://github.com/TrogonStack/t3code/pull/38)
- **0031** [Harness telemetry says T3 Code launched it](./0031-harness-telemetry-says-t3code-launched-it.md)
  submitted, [#97](https://github.com/TrogonStack/t3code/pull/97)
- **0032** [The model picker reopens where you last picked from](./0032-model-picker-reopens-where-you-left-it.md)
  active, [#103](https://github.com/TrogonStack/t3code/pull/103)
- **0033** [More secret stores](./0033-more-secret-stores.md)
  active, [#106](https://github.com/TrogonStack/t3code/pull/106)
- **0034** [A pull request watch remembers your instructions](./0034-a-pull-request-watch-remembers-your-instructions.md)
  active, [#179](https://github.com/TrogonStack/t3code/pull/179)
- **0035** [A server call's trace starts where the client made it](./0035-rpc-traces-start-in-the-client.md)
  active, [#67](https://github.com/TrogonStack/t3code/pull/67)
- **0036** [A failed commit shows what git said](./0036-a-failed-commit-shows-git-output.md)
  active, [#87](https://github.com/TrogonStack/t3code/pull/87)
- **0037** [Generated files start folded in a pull request's diff](./0037-generated-files-start-folded.md)
  active, [#219](https://github.com/TrogonStack/t3code/pull/219)
- **0038** [Wide tables in a reply stay readable](./0038-wide-chat-tables-stay-readable.md)
  active, [#236](https://github.com/TrogonStack/t3code/pull/236)
- **0039** [Turn, session, and command metrics are recorded again](./0039-orchestrator-metrics-come-back.md)
  active, [#76](https://github.com/TrogonStack/t3code/pull/76)
- **0040** [A lost provider session only stops its own work](./0040-a-released-session-leaves-other-work-alone.md)
  active, [#77](https://github.com/TrogonStack/t3code/pull/77)
- **0041** [Product telemetry is opt-in](./0041-product-telemetry-is-opt-in.md)
  active, [#252](https://github.com/TrogonStack/t3code/pull/252)
- **0042** [MCP apps with nothing to show leave no empty box](./0042-empty-mcp-apps-leave-no-box.md)
  active, [#235](https://github.com/TrogonStack/t3code/pull/235)
- **0043** [Escape returns focus to the composer](./0043-escape-returns-to-the-composer.md)
  active, [#169](https://github.com/TrogonStack/t3code/pull/169)
- **0044** [Dependabot leaves vendored sources alone](./0044-dependabot-leaves-vendored-sources-alone.md)
  active, [#256](https://github.com/TrogonStack/t3code/pull/256)
