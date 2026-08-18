import { describe, expect } from "vite-plus/test";
import { it } from "@effect/vitest";

import * as Clock from "effect/Clock";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as Ref from "effect/Ref";
import * as TestClock from "effect/testing/TestClock";
import type * as EffectAcpSchema from "effect-acp/schema";

import {
  extractModelConfigId,
  mergeToolCallState,
  parsePermissionRequest,
  parseSessionModeState,
  parseSessionUpdateEvent,
  sessionUpdateIsReplay,
  syntheticLoadSessionResponseFromInitialize,
  waitForPromptStreamStall,
  type PromptStreamActivity,
} from "./AcpRuntimeModel.ts";

const makeActivityRef = (overrides?: Partial<PromptStreamActivity>) =>
  Effect.gen(function* () {
    const lastActivityAtMillis = yield* Clock.currentTimeMillis;
    return yield* Ref.make<PromptStreamActivity>({
      lastActivityAtMillis,
      inFlightClientRequests: 0,
      ...overrides,
    });
  });

describe("AcpRuntimeModel", () => {
  it("parses session mode state from typed ACP session setup responses", () => {
    const modeState = parseSessionModeState({
      sessionId: "session-1",
      modes: {
        currentModeId: " code ",
        availableModes: [
          { id: " ask ", name: " Ask ", description: " Request approval " },
          { id: " code ", name: " Code " },
        ],
      },
      configOptions: [],
    } satisfies EffectAcpSchema.NewSessionResponse);

    expect(modeState).toEqual({
      currentModeId: "code",
      availableModes: [
        { id: "ask", name: "Ask", description: "Request approval" },
        { id: "code", name: "Code" },
      ],
    });
  });

  it("extracts the model config id from typed ACP config options", () => {
    const modelConfigId = extractModelConfigId({
      sessionId: "session-1",
      configOptions: [
        {
          id: "approval",
          name: "Approval Mode",
          category: "permission",
          type: "select",
          currentValue: "ask",
          options: [{ value: "ask", name: "Ask" }],
        },
        {
          id: "model",
          name: "Model",
          category: "model",
          type: "select",
          currentValue: "default",
          options: [{ value: "default", name: "Auto" }],
        },
      ],
    } satisfies EffectAcpSchema.NewSessionResponse);

    expect(modelConfigId).toBe("model");
  });

  it("detects Grok session replay updates from _meta.isReplay", () => {
    expect(
      sessionUpdateIsReplay({
        _meta: { isReplay: true },
        sessionId: "session-1",
        update: {
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "replayed" },
        },
      } satisfies EffectAcpSchema.SessionNotification),
    ).toBe(true);
    expect(
      sessionUpdateIsReplay({
        sessionId: "session-1",
        update: {
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "live" },
        },
      } satisfies EffectAcpSchema.SessionNotification),
    ).toBe(false);
  });

  it("builds a synthetic load response from initialize model state", () => {
    const response = syntheticLoadSessionResponseFromInitialize({
      protocolVersion: 1,
      _meta: {
        modelState: {
          currentModelId: "grok-build",
          availableModels: [{ modelId: "grok-build", name: "Grok Build" }],
        },
      },
    } satisfies EffectAcpSchema.InitializeResponse);

    expect(response.models?.currentModelId).toBe("grok-build");
    expect(response._meta).toMatchObject({ t3SessionLoadReady: "replay_idle" });
  });

  it("accepts initialize model descriptions with null", () => {
    const response = syntheticLoadSessionResponseFromInitialize({
      protocolVersion: 1,
      _meta: {
        modelState: {
          currentModelId: "grok-build",
          availableModels: [{ modelId: "grok-build", name: "Grok Build", description: null }],
        },
      },
    } satisfies EffectAcpSchema.InitializeResponse);

    expect(response.models?.availableModels[0]?.description).toBeNull();
  });

  it("ignores malformed initialize model state in synthetic load responses", () => {
    const response = syntheticLoadSessionResponseFromInitialize({
      protocolVersion: 1,
      _meta: {
        modelState: {
          currentModelId: "grok-build",
          availableModels: [null],
        },
        modeState: {
          currentModeId: "code",
          availableModes: [{ id: "code", name: 12 }],
        },
      },
    } as EffectAcpSchema.InitializeResponse);

    expect(response.models).toBeUndefined();
    expect(response.modes).toBeUndefined();
    expect(response._meta).toMatchObject({ t3SessionLoadReady: "replay_idle" });
  });

  it("builds a synthetic load response with initialize mode state", () => {
    const response = syntheticLoadSessionResponseFromInitialize({
      protocolVersion: 1,
      _meta: {
        modeState: {
          currentModeId: "code",
          availableModes: [
            { id: "ask", name: "Ask" },
            { id: "code", name: "Code" },
          ],
        },
      },
    } satisfies EffectAcpSchema.InitializeResponse);

    expect(response.modes?.currentModeId).toBe("code");
    expect(response.modes?.availableModes).toHaveLength(2);
  });

  it("projects typed ACP tool call updates into runtime events", () => {
    const created = parseSessionUpdateEvent({
      sessionId: "session-1",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "Terminal",
        kind: "execute",
        status: "pending",
        rawInput: {
          executable: "bun",
          args: ["run", "typecheck"],
        },
        content: [
          {
            type: "content",
            content: {
              type: "text",
              text: "Running checks",
            },
          },
        ],
      },
    } satisfies EffectAcpSchema.SessionNotification);

    expect(created.events).toEqual([
      {
        _tag: "ToolCallUpdated",
        toolCall: {
          toolCallId: "tool-1",
          kind: "execute",
          title: "Ran command",
          status: "pending",
          command: "bun run typecheck",
          detail: "bun run typecheck",
          data: {
            toolCallId: "tool-1",
            kind: "execute",
            command: "bun run typecheck",
            rawInput: {
              executable: "bun",
              args: ["run", "typecheck"],
            },
            content: [
              {
                type: "content",
                content: {
                  type: "text",
                  text: "Running checks",
                },
              },
            ],
          },
        },
        rawPayload: {
          sessionId: "session-1",
          update: {
            sessionUpdate: "tool_call",
            toolCallId: "tool-1",
            title: "Terminal",
            kind: "execute",
            status: "pending",
            rawInput: {
              executable: "bun",
              args: ["run", "typecheck"],
            },
            content: [
              {
                type: "content",
                content: {
                  type: "text",
                  text: "Running checks",
                },
              },
            ],
          },
        },
      },
    ]);

    const updated = parseSessionUpdateEvent({
      sessionId: "session-1",
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        status: "completed",
        rawOutput: { exitCode: 0 },
      },
    } satisfies EffectAcpSchema.SessionNotification);

    expect(updated.events).toHaveLength(1);
    expect(updated.events[0]?._tag).toBe("ToolCallUpdated");
    const createdEvent = created.events[0];
    const updatedEvent = updated.events[0];
    if (createdEvent?._tag === "ToolCallUpdated" && updatedEvent?._tag === "ToolCallUpdated") {
      expect(mergeToolCallState(createdEvent.toolCall, updatedEvent.toolCall)).toMatchObject({
        toolCallId: "tool-1",
        status: "completed",
        title: "Ran command",
        detail: "bun run typecheck",
        command: "bun run typecheck",
      });
    }
  });

  it("trims padded current mode updates before emitting a mode change", () => {
    const result = parseSessionUpdateEvent({
      sessionId: "session-1",
      update: {
        sessionUpdate: "current_mode_update",
        currentModeId: " code ",
      },
    } satisfies EffectAcpSchema.SessionNotification);

    expect(result.modeId).toBe("code");
    expect(result.events).toEqual([
      {
        _tag: "ModeChanged",
        modeId: "code",
      },
    ]);
  });

  it("projects typed ACP plan and content updates", () => {
    const planResult = parseSessionUpdateEvent({
      sessionId: "session-1",
      update: {
        sessionUpdate: "plan",
        entries: [
          { content: " Inspect state ", priority: "high", status: "completed" },
          { content: "", priority: "medium", status: "in_progress" },
        ],
      },
    } satisfies EffectAcpSchema.SessionNotification);

    expect(planResult.events).toEqual([
      {
        _tag: "PlanUpdated",
        payload: {
          plan: [
            { step: "Inspect state", status: "completed" },
            { step: "Step 2", status: "inProgress" },
          ],
        },
        rawPayload: {
          sessionId: "session-1",
          update: {
            sessionUpdate: "plan",
            entries: [
              { content: " Inspect state ", priority: "high", status: "completed" },
              { content: "", priority: "medium", status: "in_progress" },
            ],
          },
        },
      },
    ]);

    const contentResult = parseSessionUpdateEvent({
      sessionId: "session-1",
      update: {
        sessionUpdate: "agent_message_chunk",
        content: {
          type: "text",
          text: "hello from acp",
        },
      },
    } satisfies EffectAcpSchema.SessionNotification);

    expect(contentResult.events).toEqual([
      {
        _tag: "ContentDelta",
        text: "hello from acp",
        rawPayload: {
          sessionId: "session-1",
          update: {
            sessionUpdate: "agent_message_chunk",
            content: {
              type: "text",
              text: "hello from acp",
            },
          },
        },
      },
    ]);
  });

  it("keeps permission request parsing compatible with loose extension payloads", () => {
    const request = parsePermissionRequest({
      sessionId: "session-1",
      options: [
        {
          optionId: "allow-once",
          name: "Allow once",
          kind: "allow_once",
        },
      ],
      toolCall: {
        toolCallId: "tool-1",
        title: "`cat package.json`",
        kind: "execute",
        status: "pending",
        content: [
          {
            type: "content",
            content: {
              type: "text",
              text: "Not in allowlist",
            },
          },
        ],
      },
    });

    expect(request).toMatchObject({
      kind: "execute",
      detail: "cat package.json",
      toolCall: {
        toolCallId: "tool-1",
        kind: "execute",
        status: "pending",
        command: "cat package.json",
      },
    });
  });
});

describe("waitForPromptStreamStall", () => {
  it.effect("reports the idle duration once the agent goes silent", () =>
    Effect.gen(function* () {
      const activityRef = yield* makeActivityRef();
      const stall = yield* Effect.forkChild(
        waitForPromptStreamStall({ activityRef, stallAfter: Duration.minutes(10) }),
      );

      yield* TestClock.adjust(Duration.minutes(11));

      const idleMillis = yield* Fiber.join(stall);
      expect(idleMillis).toBeGreaterThanOrEqual(Duration.toMillis(Duration.minutes(10)));
    }).pipe(Effect.provide(TestClock.layer())),
  );

  it.effect("stays quiet while a client request is still in flight", () =>
    Effect.gen(function* () {
      // A slow build behind terminal/wait_for_exit: the agent is waiting on us.
      const activityRef = yield* makeActivityRef({ inFlightClientRequests: 1 });
      const stall = yield* Effect.forkChild(
        waitForPromptStreamStall({ activityRef, stallAfter: Duration.minutes(10) }),
      );

      yield* TestClock.adjust(Duration.minutes(45));

      expect(stall.pollUnsafe()).toBeUndefined();
      yield* Fiber.interrupt(stall);
    }).pipe(Effect.provide(TestClock.layer())),
  );

  it.effect("stays quiet while the agent keeps streaming", () =>
    Effect.gen(function* () {
      const activityRef = yield* makeActivityRef();
      const stall = yield* Effect.forkChild(
        waitForPromptStreamStall({ activityRef, stallAfter: Duration.minutes(10) }),
      );

      for (let tick = 0; tick < 6; tick += 1) {
        yield* TestClock.adjust(Duration.minutes(9));
        const lastActivityAtMillis = yield* Clock.currentTimeMillis;
        yield* Ref.update(activityRef, (activity) => ({ ...activity, lastActivityAtMillis }));
      }

      expect(stall.pollUnsafe()).toBeUndefined();
      yield* Fiber.interrupt(stall);
    }).pipe(Effect.provide(TestClock.layer())),
  );

  it.effect("reports a stall once the last client request settles", () =>
    Effect.gen(function* () {
      const activityRef = yield* makeActivityRef({ inFlightClientRequests: 1 });
      const stall = yield* Effect.forkChild(
        waitForPromptStreamStall({ activityRef, stallAfter: Duration.minutes(10) }),
      );

      yield* TestClock.adjust(Duration.minutes(30));
      expect(stall.pollUnsafe()).toBeUndefined();

      const lastActivityAtMillis = yield* Clock.currentTimeMillis;
      yield* Ref.set(activityRef, { lastActivityAtMillis, inFlightClientRequests: 0 });
      yield* TestClock.adjust(Duration.minutes(11));

      const idleMillis = yield* Fiber.join(stall);
      expect(idleMillis).toBeGreaterThanOrEqual(Duration.toMillis(Duration.minutes(10)));
    }).pipe(Effect.provide(TestClock.layer())),
  );
});
