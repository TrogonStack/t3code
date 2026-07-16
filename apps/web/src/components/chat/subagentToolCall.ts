import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

const SPAWN_TOOL_NAME = "mcp__t3-code__spawn_thread";
const AWAIT_TOOL_NAME = "mcp__t3-code__await_thread";

const SpawnResultContent = Schema.fromJsonString(
  Schema.Struct({
    threadId: Schema.String,
    title: Schema.optional(Schema.String),
  }),
);

const AwaitResultContent = Schema.fromJsonString(
  Schema.Struct({
    threadId: Schema.String,
    status: Schema.Literals(["completed", "failed", "interrupted", "timeout"]),
    finalMessage: Schema.NullOr(Schema.String),
  }),
);

const decodeSpawnResult = Schema.decodeUnknownOption(SpawnResultContent);
const decodeAwaitResult = Schema.decodeUnknownOption(AwaitResultContent);

export type SubagentToolCallStatus =
  | "pending"
  | "spawned"
  | "completed"
  | "failed"
  | "interrupted"
  | "timeout";

export interface SubagentToolCallView {
  readonly kind: "spawn" | "await";
  readonly heading: string;
  readonly status: SubagentToolCallStatus;
  readonly threadId: string | null;
  readonly title: string | null;
}

const asRecord = (value: unknown): Record<string, unknown> | undefined =>
  typeof value === "object" && value !== null ? (value as Record<string, unknown>) : undefined;

const resultContent = (toolData: Record<string, unknown>): unknown => {
  const result = asRecord(toolData.result);
  return result?.content;
};

const headingForAwaitStatus = (status: SubagentToolCallStatus): string => {
  switch (status) {
    case "completed":
      return "Subagent finished";
    case "failed":
      return "Subagent failed";
    case "interrupted":
      return "Subagent interrupted";
    case "timeout":
      return "Subagent still running";
    default:
      return "Waiting for subagent";
  }
};

/**
 * Recognize the t3-code subagent MCP tool calls in a timeline work entry so
 * they render as first-class subagent rows instead of raw MCP payloads.
 */
export function parseSubagentToolCall(workEntry: {
  readonly itemType?: string;
  readonly toolData?: unknown;
}): SubagentToolCallView | null {
  if (workEntry.itemType !== "mcp_tool_call") {
    return null;
  }
  const toolData = asRecord(workEntry.toolData);
  if (toolData === undefined) {
    return null;
  }
  const input = asRecord(toolData.input);

  if (toolData.toolName === SPAWN_TOOL_NAME) {
    const inputTitle = typeof input?.title === "string" ? input.title : null;
    if (toolData.result === undefined) {
      return {
        kind: "spawn",
        heading: "Spawning subagent",
        status: "pending",
        threadId: null,
        title: inputTitle,
      };
    }
    return Option.match(decodeSpawnResult(resultContent(toolData)), {
      onNone: () => ({
        kind: "spawn" as const,
        heading: "Subagent spawn failed",
        status: "failed" as const,
        threadId: null,
        title: inputTitle,
      }),
      onSome: (decoded) => ({
        kind: "spawn" as const,
        heading: "Spawned subagent",
        status: "spawned" as const,
        threadId: decoded.threadId,
        title: decoded.title ?? inputTitle,
      }),
    });
  }

  if (toolData.toolName === AWAIT_TOOL_NAME) {
    const inputThreadId = typeof input?.threadId === "string" ? input.threadId : null;
    if (toolData.result === undefined) {
      return {
        kind: "await",
        heading: headingForAwaitStatus("pending"),
        status: "pending",
        threadId: inputThreadId,
        title: null,
      };
    }
    return Option.match(decodeAwaitResult(resultContent(toolData)), {
      onNone: () => ({
        kind: "await" as const,
        heading: headingForAwaitStatus("failed"),
        status: "failed" as const,
        threadId: inputThreadId,
        title: null,
      }),
      onSome: (decoded) => ({
        kind: "await" as const,
        heading: headingForAwaitStatus(decoded.status),
        status: decoded.status,
        threadId: decoded.threadId,
        title: null,
      }),
    });
  }

  return null;
}
