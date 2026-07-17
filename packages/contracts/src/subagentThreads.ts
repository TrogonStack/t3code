import * as Schema from "effect/Schema";

import { NonNegativeInt, ThreadId, TrimmedNonEmptyString } from "./baseSchemas.ts";
import { ProviderInstanceId } from "./providerInstance.ts";
import { RuntimeMode } from "./orchestration.ts";

export const SUBAGENT_MAX_DEPTH = 5;
export const SUBAGENT_MAX_RUNNING_PER_TREE = 8;
export const SUBAGENT_AWAIT_DEFAULT_TIMEOUT_SECONDS = 600;

export const SubagentEnvMode = Schema.Literals(["worktree", "local"]);
export type SubagentEnvMode = typeof SubagentEnvMode.Type;

export const SpawnThreadInput = Schema.Struct({
  prompt: Schema.String.check(Schema.isTrimmed()).check(
    Schema.isNonEmpty({
      description:
        "The full task for the subagent. It starts with no other context, so include everything it needs: goal, constraints, and what to report back.",
    }),
  ),
  title: Schema.optional(
    TrimmedNonEmptyString.annotate({
      description:
        "Short human-readable title for the subagent thread. Derived from the prompt when omitted.",
    }),
  ).annotate({
    description:
      "Short human-readable title for the subagent thread. Derived from the prompt when omitted.",
  }),
  providerInstanceId: Schema.optional(
    ProviderInstanceId.annotate({
      description:
        "Provider instance to run the subagent on. Defaults to this thread's provider. When set to a different provider, model must also be set.",
    }),
  ).annotate({
    description:
      "Provider instance to run the subagent on. Defaults to this thread's provider. When set to a different provider, model must also be set.",
  }),
  model: Schema.optional(
    TrimmedNonEmptyString.annotate({
      description:
        "Model for the subagent. Defaults to this thread's model when the provider is unchanged.",
    }),
  ).annotate({
    description:
      "Model for the subagent. Defaults to this thread's model when the provider is unchanged.",
  }),
  envMode: Schema.optional(
    SubagentEnvMode.annotate({
      description:
        'Workspace isolation: "worktree" (default) gives the subagent its own git worktree; "local" shares this thread\'s checkout and suits read-only tasks.',
    }),
  ).annotate({
    description:
      'Workspace isolation: "worktree" (default) gives the subagent its own git worktree; "local" shares this thread\'s checkout and suits read-only tasks.',
  }),
  runtimeMode: Schema.optional(
    RuntimeMode.annotate({
      description: "Permission mode for the subagent. Defaults to this thread's runtime mode.",
    }),
  ).annotate({
    description: "Permission mode for the subagent. Defaults to this thread's runtime mode.",
  }),
});
export type SpawnThreadInput = typeof SpawnThreadInput.Type;

export const SpawnThreadResult = Schema.Struct({
  threadId: ThreadId,
  title: TrimmedNonEmptyString,
});
export type SpawnThreadResult = typeof SpawnThreadResult.Type;

export const AwaitThreadInput = Schema.Struct({
  threadId: Schema.String.check(
    Schema.isNonEmpty({ description: "Thread id returned by spawn_thread." }),
  ),
  timeoutSeconds: Schema.optional(
    NonNegativeInt.annotate({
      description:
        "Maximum seconds to wait. On timeout the subagent keeps running and can be awaited again.",
    }),
  ).annotate({
    description:
      "Maximum seconds to wait. On timeout the subagent keeps running and can be awaited again.",
  }),
});
export type AwaitThreadInput = typeof AwaitThreadInput.Type;

export const AwaitThreadStatus = Schema.Literals(["completed", "failed", "interrupted", "timeout"]);
export type AwaitThreadStatus = typeof AwaitThreadStatus.Type;

export const AwaitThreadResult = Schema.Struct({
  threadId: ThreadId,
  status: AwaitThreadStatus,
  finalMessage: Schema.NullOr(Schema.String),
});
export type AwaitThreadResult = typeof AwaitThreadResult.Type;

export class SubagentThreadError extends Schema.TaggedErrorClass<SubagentThreadError>()(
  "SubagentThreadError",
  {
    reason: Schema.Literals([
      "not_permitted",
      "depth_exceeded",
      "concurrency_exceeded",
      "unknown_provider",
      "unknown_thread",
      "not_a_child",
      "spawn_failed",
    ]),
    detail: TrimmedNonEmptyString,
  },
) {
  override get message(): string {
    return this.detail;
  }
}
