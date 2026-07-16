# 0003: Native subagent threads for Claude orchestrators

- PR: [TrogonStack/t3code#3](https://github.com/TrogonStack/t3code/pull/3)
- Status: active

## What you can do now

- A Claude thread can spawn subagents that run as real T3 Code threads in the
  same project. Ask a Claude thread to delegate work and each subagent
  appears as its own thread: openable, inspectable mid-flight, steerable,
  and resumable after the orchestrator is done.
- Claude's built-in invisible subagents are redirected to this mechanism by
  default, so delegation no longer disappears inside the CLI. A per-instance
  provider setting ("Route subagents through T3 Code") turns the redirect
  off.
- Subagents can run on any provider and model, not just the parent's: a
  Claude orchestrator can fan work out to Codex or cheaper Claude children.
- Each subagent defaults to its own git worktree (with the project setup
  script), so parallel children do not trample one checkout; read-only tasks
  can opt into sharing the parent's checkout.
- The parent thread's header shows a live subagent chip (count, running
  indicator, jump-to-child menu), and each child shows a "spawned by" chip
  linking back to its orchestrator.
- Guardrails: subagents cannot spawn further subagents, and at most four may
  run per parent at once.

## Why

Provider CLIs run subagents internally where they are unobservable by
construction: no transcript, no intervention, no reuse. Running delegation
through T3 Code itself makes subagents first-class product objects instead
of an opaque tool call, and unlocks cross-provider orchestration that no
single CLI can do.

## Upstream considerations

Built on the existing MCP server, thread orchestration, and worktree
machinery; the largest reusable piece (extracting thread bootstrap into a
shared service) is a clean refactor upstream may want regardless. The
feature itself is a product direction upstream has not signaled, so expect
this to remain a fork divergence. Extending parenting beyond Claude only
needs per-provider native-tool redirection; children already work on every
provider.
