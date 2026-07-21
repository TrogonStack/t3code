import {
  CommandId,
  DEFAULT_PROVIDER_INTERACTION_MODE,
  MessageId,
  ProjectId,
  ProviderInstanceId,
  ThreadId,
  TurnId,
  type OrchestrationCommand,
  type OrchestrationThread,
  type ServerSettingsError,
} from "@t3tools/contracts";
import { assert, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as Ref from "effect/Ref";

import { ProviderService } from "../../provider/Services/ProviderService.ts";
import { ServerSettingsService } from "../../serverSettings.ts";
import { TextGeneration } from "../../textGeneration/TextGeneration.ts";
import { renderTranscript } from "../../textGeneration/TextGenerationPrompts.ts";
import { OrchestrationEngineService } from "../Services/OrchestrationEngine.ts";
import { ProjectionSnapshotQuery } from "../Services/ProjectionSnapshotQuery.ts";
import { ThreadForkService } from "../Services/ThreadFork.ts";
import { ThreadForkLive } from "./ThreadFork.ts";

const asThreadId = (value: string): ThreadId => ThreadId.make(value);
const asMessageId = (value: string): MessageId => MessageId.make(value);
const asTurnId = (value: string): TurnId => TurnId.make(value);
const asProjectId = (value: string): ProjectId => ProjectId.make(value);
const asCommandId = (value: string): CommandId => CommandId.make(value);

const now = "2026-01-01T00:00:00.000Z";

const modelSelection = {
  instanceId: ProviderInstanceId.make("codex"),
  model: "gpt-5-codex",
};

function makeSourceThread(): OrchestrationThread {
  return {
    id: asThreadId("thread-source"),
    projectId: asProjectId("project-1"),
    title: "Source Thread",
    modelSelection,
    runtimeMode: "approval-required",
    interactionMode: DEFAULT_PROVIDER_INTERACTION_MODE,
    branch: null,
    worktreePath: null,
    parentThreadId: null,
    latestTurn: null,
    createdAt: now,
    updatedAt: now,
    archivedAt: null,
    deletedAt: null,
    messages: [
      {
        id: asMessageId("message-1"),
        role: "user",
        text: "First message",
        turnId: asTurnId("turn-1"),
        streaming: false,
        createdAt: now,
        updatedAt: now,
      },
      {
        id: asMessageId("message-2"),
        role: "assistant",
        text: "Second message",
        turnId: asTurnId("turn-2"),
        streaming: false,
        createdAt: now,
        updatedAt: now,
      },
    ],
    proposedPlans: [],
    activities: [],
    checkpoints: [],
    session: null,
  };
}

type ForkConversationCall = {
  readonly sourceThreadId: ThreadId;
  readonly newThreadId: ThreadId;
  readonly upToMessageId: MessageId | null;
  readonly upToTurnId: TurnId | null;
};

type DispatchedFinalizeCommand = Extract<OrchestrationCommand, { type: "thread.fork.finalize" }>;

interface TestHarness {
  readonly testLayer: Layer.Layer<ThreadForkService, ServerSettingsError>;
  readonly dispatchedCommands: Ref.Ref<ReadonlyArray<OrchestrationCommand>>;
  readonly forkConversationCalls: Ref.Ref<ReadonlyArray<ForkConversationCall>>;
}

function makeHarness(input: {
  readonly sourceThread: Option.Option<OrchestrationThread>;
  readonly forkConversationResult:
    | { readonly supported: true; readonly resumeCursor: unknown }
    | { readonly supported: false };
  readonly summaryText?: string;
}): Effect.Effect<TestHarness> {
  return Effect.gen(function* () {
    const dispatchedCommands = yield* Ref.make<ReadonlyArray<OrchestrationCommand>>([]);
    const forkConversationCalls = yield* Ref.make<ReadonlyArray<ForkConversationCall>>([]);

    const orchestrationEngineLayer = Layer.mock(OrchestrationEngineService)({
      dispatch: (command) =>
        Ref.update(dispatchedCommands, (commands) => [...commands, command]).pipe(
          Effect.as({ sequence: 1 }),
        ),
    });

    const projectionSnapshotQueryLayer = Layer.mock(ProjectionSnapshotQuery)({
      getThreadDetailById: () => Effect.succeed(input.sourceThread),
      getProjectShellById: () => Effect.succeed(Option.none()),
    });

    const providerServiceLayer = Layer.mock(ProviderService)({
      forkConversation: (forkInput) =>
        Ref.update(forkConversationCalls, (calls) => [...calls, forkInput]).pipe(
          Effect.as(input.forkConversationResult),
        ),
    });

    const textGenerationLayer = Layer.mock(TextGeneration)({
      generateThreadForkSummary: () =>
        Effect.succeed({ summary: input.summaryText ?? "unexpected summary call" }),
    });

    const serverSettingsLayer = ServerSettingsService.layerTest();

    const testLayer = ThreadForkLive.pipe(
      Layer.provide(
        Layer.mergeAll(
          orchestrationEngineLayer,
          projectionSnapshotQueryLayer,
          providerServiceLayer,
          textGenerationLayer,
          serverSettingsLayer,
        ),
      ),
    );

    return { testLayer, dispatchedCommands, forkConversationCalls };
  });
}

function makeForkCommand(
  overrides: Partial<Extract<OrchestrationCommand, { type: "thread.fork" }>> = {},
): Extract<OrchestrationCommand, { type: "thread.fork" }> {
  return {
    type: "thread.fork",
    commandId: asCommandId("cmd-thread-fork"),
    sourceThreadId: asThreadId("thread-source"),
    newThreadId: asThreadId("thread-new"),
    mode: "full-history",
    createdAt: now,
    ...overrides,
  };
}

it.effect(
  "summary mode dispatches a summary finalize command without calling forkConversation",
  () =>
    Effect.gen(function* () {
      const sourceThread = makeSourceThread();
      const harness = yield* makeHarness({
        sourceThread: Option.some(sourceThread),
        forkConversationResult: { supported: false },
        summaryText: "You were working on X.",
      });

      const dispatched = yield* Effect.gen(function* () {
        const threadFork = yield* ThreadForkService;
        yield* threadFork.dispatchFork(makeForkCommand({ mode: "summary" }));
        return yield* Ref.get(harness.dispatchedCommands);
      }).pipe(Effect.provide(harness.testLayer));

      const calls = yield* Ref.get(harness.forkConversationCalls);

      assert.strictEqual(dispatched.length, 1);
      const command = dispatched[0] as DispatchedFinalizeCommand;
      assert.strictEqual(command.type, "thread.fork.finalize");
      assert.strictEqual(command.mode, "summary");
      assert.strictEqual(command.summaryText, "You were working on X.");
      assert.strictEqual(command.pendingForkContextText, "You were working on X.");
      assert.deepStrictEqual(calls, []);
    }),
);

it.effect("full-history mode with native fork support clears pendingForkContextText", () =>
  Effect.gen(function* () {
    const sourceThread = makeSourceThread();
    const harness = yield* makeHarness({
      sourceThread: Option.some(sourceThread),
      forkConversationResult: { supported: true, resumeCursor: "cursor" as unknown },
    });

    const dispatched = yield* Effect.gen(function* () {
      const threadFork = yield* ThreadForkService;
      yield* threadFork.dispatchFork(makeForkCommand());
      return yield* Ref.get(harness.dispatchedCommands);
    }).pipe(Effect.provide(harness.testLayer));

    const command = dispatched[0] as DispatchedFinalizeCommand;
    assert.strictEqual(command.mode, "full-history");
    assert.strictEqual(command.pendingForkContextText, null);
  }),
);

it.effect("full-history mode without native fork support falls back to a rendered transcript", () =>
  Effect.gen(function* () {
    const sourceThread = makeSourceThread();
    const harness = yield* makeHarness({
      sourceThread: Option.some(sourceThread),
      forkConversationResult: { supported: false },
    });

    const dispatched = yield* Effect.gen(function* () {
      const threadFork = yield* ThreadForkService;
      yield* threadFork.dispatchFork(makeForkCommand());
      return yield* Ref.get(harness.dispatchedCommands);
    }).pipe(Effect.provide(harness.testLayer));

    const command = dispatched[0] as DispatchedFinalizeCommand;
    assert.strictEqual(command.mode, "full-history");
    assert.strictEqual(command.pendingForkContextText, renderTranscript(sourceThread.messages));
  }),
);

it.effect("threads upToTurnId from the cutoff message through to forkConversation", () =>
  Effect.gen(function* () {
    const sourceThread = makeSourceThread();
    const harness = yield* makeHarness({
      sourceThread: Option.some(sourceThread),
      forkConversationResult: { supported: false },
    });

    yield* Effect.gen(function* () {
      const threadFork = yield* ThreadForkService;
      yield* threadFork.dispatchFork(makeForkCommand({ upToMessageId: asMessageId("message-1") }));
    }).pipe(Effect.provide(harness.testLayer));

    const calls = yield* Ref.get(harness.forkConversationCalls);
    assert.strictEqual(calls.length, 1);
    assert.strictEqual(calls[0]?.upToMessageId, asMessageId("message-1"));
    assert.strictEqual(calls[0]?.upToTurnId, asTurnId("turn-1"));
  }),
);

it.effect("fails when the source thread cannot be found", () =>
  Effect.gen(function* () {
    const harness = yield* makeHarness({
      sourceThread: Option.none(),
      forkConversationResult: { supported: false },
    });

    const error = yield* Effect.gen(function* () {
      const threadFork = yield* ThreadForkService;
      return yield* threadFork.dispatchFork(makeForkCommand()).pipe(Effect.flip);
    }).pipe(Effect.provide(harness.testLayer));

    assert.strictEqual(error._tag, "OrchestrationDispatchCommandError");
    assert.match(error.message, /was not found/);
  }),
);

it.effect("fails when upToMessageId does not exist on the source thread", () =>
  Effect.gen(function* () {
    const sourceThread = makeSourceThread();
    const harness = yield* makeHarness({
      sourceThread: Option.some(sourceThread),
      forkConversationResult: { supported: false },
    });

    const error = yield* Effect.gen(function* () {
      const threadFork = yield* ThreadForkService;
      return yield* threadFork
        .dispatchFork(makeForkCommand({ upToMessageId: asMessageId("message-does-not-exist") }))
        .pipe(Effect.flip);
    }).pipe(Effect.provide(harness.testLayer));

    assert.strictEqual(error._tag, "OrchestrationDispatchCommandError");
    assert.match(error.message, /does not exist on the source thread/);
  }),
);
