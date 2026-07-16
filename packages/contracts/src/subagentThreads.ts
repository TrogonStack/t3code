import * as Schema from "effect/Schema";

import { NonNegativeInt, ThreadId, TrimmedNonEmptyString } from "./baseSchemas.ts";
import { ProviderInstanceId } from "./providerInstance.ts";
import { RuntimeMode } from "./orchestration.ts";

export const SUBAGENT_MAX_RUNNING_CHILDREN = 4;
export const SUBAGENT_AWAIT_DEFAULT_TIMEOUT_SECONDS = 600;

export const SubagentEnvMode = Schema.Literals(["worktree", "local"]);
export type SubagentEnvMode = typeof SubagentEnvMode.Type;

export const SpawnThreadInput = Schema.Struct({
  prompt: TrimmedNonEmptyString,
  title: Schema.optional(TrimmedNonEmptyString),
  providerInstanceId: Schema.optional(ProviderInstanceId),
  model: Schema.optional(TrimmedNonEmptyString),
  envMode: Schema.optional(SubagentEnvMode),
  runtimeMode: Schema.optional(RuntimeMode),
});
export type SpawnThreadInput = typeof SpawnThreadInput.Type;

export const SpawnThreadResult = Schema.Struct({
  threadId: ThreadId,
  title: TrimmedNonEmptyString,
});
export type SpawnThreadResult = typeof SpawnThreadResult.Type;

export const AwaitThreadInput = Schema.Struct({
  threadId: ThreadId,
  timeoutSeconds: Schema.optional(NonNegativeInt),
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
