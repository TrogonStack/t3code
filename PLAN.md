# Durable workflow runs

## Why

When a user fans work out to several agents today, the plan lives in the coordinating agent's
context. That context gets compacted, the coordinator rebuilds status by reading git, open
questions are repeated in every reply, and a server restart cancels background work that "will not
report back". Users ask "where are we?" and "is it stuck?" because nothing durable answers them.

A real session showed the shape clearly: snapshot a shared base, run five lanes in parallel (each
an implement then verify loop), merge, run the full suite, push, with two user decisions
outstanding the whole time. Every piece of that was held together by the model and by hand-written
notes.

Kiro's "Introducing Kiro workflows" post and the TrogonStack/trogonai#592 workflow contracts
describe the fix: the model proposes the plan, the runtime owns its execution, and each step runs in
a fresh session.

## Goal

A coordinating thread can start a workflow: a small tree of steps that T3 executes durably through
the existing delegated-task machinery. T3 owns sequencing, joins, loops, retries and status. The
user can see where a run is, pause it, cancel it, answer its questions and revise what has not
started yet, from any client and across server restarts.

## Non-goals

- Porting trogonai#592 wholesale. Borrow its vocabulary and invariants, not its surface area.
- Orchestrating provider-native subagents. They stay observed-only mirror threads.
- External watchers, arbitrary predicates, or a recipe marketplace in the first cut.
- A second agent runtime. Steps are ordinary delegated child threads.

## What already exists

| Need                                     | Existing piece                                                                                            |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Pure decisions, events, receipts, outbox | `Orchestrator.ts`, `EventSink.commitCommand`, `EffectWorker`                                              |
| A step in a fresh session                | `delegate_task` MCP tool -> `delegated_task.request` -> child thread (`relationshipToParent: "subagent"`) |
| Results back to the coordinator          | Delegated completion delivery mailbox, `ProviderContinuationService`                                      |
| Retry identity                           | `RunAttempt` reasons (`retry`, `provider_recovery`, ...)                                                  |
| Asking the user                          | Runtime requests (`user_input`, approvals), `RuntimeRequestService`                                       |
| Steering a step                          | `message.dispatch` modes (`steer_active`, `queue_after_active`)                                           |
| Waiting on a PR                          | `PullRequestWatchReactor`                                                                                 |
| Child cannot exceed parent's runtime     | `OrchestratorMcpService` guardrail                                                                        |

What is missing: a declared plan, a run record that owns it, and recovery that reconciles
dispatched steps instead of retiring them and asking the model to "continue where you left off".

## Proposed model

Smallest model that makes the behavior unsurprising:

- **WorkflowDefinition** (contracts): flat named nodes plus a root.
  - `step`: task prompt, optional title, optional model selection (defaults to the coordinator's), declared outputs.
  - `sequence`: ordered children.
  - `parallel`: children plus join `all` or `first_success`.
  - `repeat`: body, `maxIterations`, stop condition on a declared step output (for example `verdict == "approved"`), limit policy `fail` or `accept_last`.
- **Workflow run**: owned by the coordinating thread, so it serializes on the existing thread key
  instead of introducing a new one. Records the exact admitted definition, node occurrences
  (node + iteration path), dispatches (child thread id, command id), step outcomes, and decisions.
- **Step outcome**: the child reports a structured result through a new MCP tool
  (`workflow_step_complete`: outcome `succeeded | partial | failed`, declared outputs, summary).
  A negative review verdict is a successful step with an output, not a failure. Fallback when the
  tool is never called: the child's settled final message with outcome `unknown`.
- **Recovery**: a dispatch that has a receipt is never dispatched again. On restart, reconcile each
  dispatched step against its child thread state. Only steps with a determinate failure are
  retryable.
- **Reverse states**: start / cancel, pause / resume, request input / answer, revise remaining
  plan (pending nodes only, at a pause boundary).

## Phases

### 0. Decide and spike

- [ ] Confirm `delegated_task.request` can be dispatched by the workflow without a live parent
      run and node (it currently requires `parentRunId` and `parentNodeId`). Decide whether the
      workflow run supplies a synthetic node, or the command grows a workflow owner.
- [ ] Decide where completion goes: the workflow decider consumes the delegated completion, and the
      coordinator's turn gets a progress message instead of the raw result.
- [ ] Confirm per-thread serialization is enough when many children complete at once.

### 1. Contracts and pure decider

- [ ] `packages/contracts`: definition schema, workflow commands and events, run projection shape.
- [ ] Orchestrator decisions for start, node entry, dispatch, step outcome, join, repeat advance,
      finish. No I/O.
- [ ] Focused tests: sequence ordering, parallel `all` and `first_success`, repeat stop and limit
      policies, duplicate outcome receipts, cancellation mid-parallel.

### 2. Effects and recovery

- [ ] Effect that dispatches a step through the existing delegated-task path with a deterministic
      command id per occurrence and attempt.
- [ ] Route child completion to the workflow decider.
- [ ] Restart reconciliation in `ProviderRuntimeRecoveryService` / `EffectOutbox`: no
      re-dispatch, reconcile against the child thread.
- [ ] Tests that drain `OrchestrationEffectWorkerV2` and await persisted events. No sleeps.

### 3. Agent surface (MCP)

- [ ] `workflow_start` (definition in, run id out), `workflow_status`, `workflow_revise`,
      `workflow_pause` / `workflow_resume` / `workflow_cancel`, `workflow_step_complete`.
- [ ] Transports stay thin: decode, call one service method, map errors.
- [ ] Per-provider decision: MCP-capable providers get the tools; others marked unsupported.

### 4. Clients

- [ ] `packages/client-runtime`: shared workflow run subscription and state.
- [ ] Web and desktop: a compact run tree on the coordinating thread (node, state, child link,
      outcome), with pause, resume, cancel and pending input. No continuously animating indicators.
- [ ] Mobile: same tree and controls in the React Native thread view.
- [ ] Command palette and keybinding entries for pause, resume, cancel.
- [ ] Send only the run summary over the wire; child detail loads on demand.

### 5. Inputs and waits

- [ ] Workflow input requests that block only dependent nodes, answerable from any client or by an
      agent through the existing pending-request tools.
- [ ] Later: PR watch as a wait node; scheduled tasks can start a saved definition.

### 6. Docs

- [ ] `docs/internals/`: why the workflow owns sequencing and why recovery reconciles instead of
      replaying. Link source, do not narrate it.
- [ ] `docs/user/`: how to start a workflow, see status, and steer it.

## Acceptance scenario

Replay the fan-out session as a workflow:

`sequence[snapshot-base, parallel(lane A..E, each repeat(implement -> verify, max 3)), merge, full-test, push]`

- Status answers "where are we?" without asking the model.
- Restarting the server mid-parallel neither loses nor repeats a lane.
- An unanswered decision blocks only the lane that needs it.
- Sending a lane back for more items is a plan revision that keeps finished steps.
- Works from a remote browser and from mobile on the same environment.

## Open decisions

- Workflow owner: coordinating thread (proposed) or a new top-level record.
- Whether saved recipes ship in the first cut or come after ad hoc workflows prove out.
- Outcome reporting: require `workflow_step_complete`, or accept settled final message as default.
- Whether this should eventually emit trogonai#592 events through an adapter.
