import {
  AwaitThreadInput,
  AwaitThreadResult,
  SpawnThreadInput,
  SpawnThreadResult,
  SUBAGENT_AWAIT_DEFAULT_TIMEOUT_SECONDS,
  SUBAGENT_MAX_DEPTH,
  SUBAGENT_MAX_RUNNING_PER_TREE,
  SubagentThreadError,
} from "@t3tools/contracts";
import * as Crypto from "effect/Crypto";
import { Tool, Toolkit } from "effect/unstable/ai";

import * as OrchestrationEngine from "../../../orchestration/Services/OrchestrationEngine.ts";
import * as ProjectionSnapshotQuery from "../../../orchestration/Services/ProjectionSnapshotQuery.ts";
import * as ThreadBootstrap from "../../../orchestration/Services/ThreadBootstrap.ts";
import * as ProviderAdapterRegistry from "../../../provider/Services/ProviderAdapterRegistry.ts";
import * as GitWorkflowService from "../../../git/GitWorkflowService.ts";
import * as McpInvocationContext from "../../McpInvocationContext.ts";

const dependencies = [
  Crypto.Crypto,
  McpInvocationContext.McpInvocationContext,
  OrchestrationEngine.OrchestrationEngineService,
  ProjectionSnapshotQuery.ProjectionSnapshotQuery,
  ThreadBootstrap.ThreadBootstrapService,
  ProviderAdapterRegistry.ProviderAdapterRegistry,
  GitWorkflowService.GitWorkflowService,
];

export const SpawnThreadTool = Tool.make("spawn_thread", {
  description: `Spawn a subagent as a new thread in the current project and start it on the given prompt. The subagent runs independently with its own context; use await_thread to collect its result. Children default to an isolated git worktree (envMode "worktree"); pass envMode "local" for read-only tasks that can share the current checkout. Provider, model, and runtime mode default to this thread's. Limits: subagents may spawn their own subagents up to ${SUBAGENT_MAX_DEPTH} levels deep, and at most ${SUBAGENT_MAX_RUNNING_PER_TREE} subagents may run at once across a tree.`,
  parameters: SpawnThreadInput,
  success: SpawnThreadResult,
  failure: SubagentThreadError,
  dependencies,
})
  .annotate(Tool.Title, "Spawn subagent thread")
  .annotate(Tool.Destructive, false)
  .annotate(Tool.OpenWorld, false);

export const AwaitThreadTool = Tool.make("await_thread", {
  description: `Wait for a spawned subagent thread to finish its current turn and return its final message. Only threads spawned by this thread can be awaited. Waits up to timeoutSeconds (default ${SUBAGENT_AWAIT_DEFAULT_TIMEOUT_SECONDS}); on timeout the subagent keeps running and can be awaited again.`,
  parameters: AwaitThreadInput,
  success: AwaitThreadResult,
  failure: SubagentThreadError,
  dependencies,
})
  .annotate(Tool.Title, "Await subagent thread")
  .annotate(Tool.Readonly, true)
  .annotate(Tool.Destructive, false)
  .annotate(Tool.OpenWorld, false);

export const ThreadsToolkit = Toolkit.make(SpawnThreadTool, AwaitThreadTool);
