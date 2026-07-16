import { describe, expect, it } from "@effect/vitest";

import { parseSubagentToolCall } from "./subagentToolCall.ts";

describe("parseSubagentToolCall", () => {
  it("ignores unrelated tool calls", () => {
    expect(parseSubagentToolCall({ itemType: "command_execution" })).toBeNull();
    expect(
      parseSubagentToolCall({
        itemType: "mcp_tool_call",
        toolData: { toolName: "mcp__t3-code__preview_status", input: {} },
      }),
    ).toBeNull();
  });

  it("parses an in-flight spawn with the input title", () => {
    const view = parseSubagentToolCall({
      itemType: "mcp_tool_call",
      toolData: {
        toolName: "mcp__t3-code__spawn_thread",
        input: { prompt: "do it", title: "Joke Writer B" },
      },
    });
    expect(view).toMatchObject({
      kind: "spawn",
      status: "pending",
      title: "Joke Writer B",
      threadId: null,
    });
  });

  it("parses a completed spawn from the result content", () => {
    const view = parseSubagentToolCall({
      itemType: "mcp_tool_call",
      toolData: {
        toolName: "mcp__t3-code__spawn_thread",
        input: { prompt: "do it" },
        result: {
          type: "tool_result",
          content: '{"threadId":"a6d4f92c-9386-473d-b0bb-887ba997223c","title":"Joke Writer B"}',
        },
      },
    });
    expect(view).toMatchObject({
      kind: "spawn",
      status: "spawned",
      threadId: "a6d4f92c-9386-473d-b0bb-887ba997223c",
      title: "Joke Writer B",
    });
  });

  it("marks a spawn whose result is an error as failed", () => {
    const view = parseSubagentToolCall({
      itemType: "mcp_tool_call",
      toolData: {
        toolName: "mcp__t3-code__spawn_thread",
        input: { prompt: "do it", title: "Joke Writer B" },
        result: { type: "tool_result", content: "Error: something exploded" },
      },
    });
    expect(view).toMatchObject({ kind: "spawn", status: "failed", title: "Joke Writer B" });
  });

  it("parses await results including timeout", () => {
    const view = parseSubagentToolCall({
      itemType: "mcp_tool_call",
      toolData: {
        toolName: "mcp__t3-code__await_thread",
        input: { threadId: "child-1" },
        result: {
          type: "tool_result",
          content: '{"threadId":"child-1","status":"timeout","finalMessage":null}',
        },
      },
    });
    expect(view).toMatchObject({ kind: "await", status: "timeout", threadId: "child-1" });
  });

  it("parses a pending await from the input", () => {
    const view = parseSubagentToolCall({
      itemType: "mcp_tool_call",
      toolData: {
        toolName: "mcp__t3-code__await_thread",
        input: { threadId: "child-1" },
      },
    });
    expect(view).toMatchObject({ kind: "await", status: "pending", threadId: "child-1" });
  });
});
