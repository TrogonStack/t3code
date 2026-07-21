import {
  CommandId,
  DEFAULT_PROVIDER_INTERACTION_MODE,
  EventId,
  MessageId,
  ProjectId,
  ProviderInstanceId,
  ThreadId,
  type OrchestrationEvent,
  type OrchestrationReadModel,
} from "@t3tools/contracts";
import * as NodeServices from "@effect/platform-node/NodeServices";
import { expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

import { decideOrchestrationCommand } from "./decider.ts";
import {
  OrchestrationCommandInvariantError,
  type OrchestrationProjectorDecodeError,
} from "./Errors.ts";
import { createEmptyReadModel, projectEvent } from "./projector.ts";

const isOrchestrationCommandInvariantError = Schema.is(OrchestrationCommandInvariantError);

function expectCommandInvariantError(error: unknown): OrchestrationCommandInvariantError {
  if (!isOrchestrationCommandInvariantError(error)) {
    throw new Error(`Expected an OrchestrationCommandInvariantError, got: ${String(error)}`);
  }
  return error;
}

const asEventId = (value: string): EventId => EventId.make(value);
const asProjectId = (value: string): ProjectId => ProjectId.make(value);
const asThreadId = (value: string): ThreadId => ThreadId.make(value);
const asMessageId = (value: string): MessageId => MessageId.make(value);

const now = "2026-01-01T00:00:00.000Z";

const seedReadModel: Effect.Effect<OrchestrationReadModel, OrchestrationProjectorDecodeError> =
  Effect.gen(function* () {
    const initial = createEmptyReadModel(now);
    const withProject = yield* projectEvent(initial, {
      sequence: 1,
      eventId: asEventId("evt-project-create"),
      aggregateKind: "project",
      aggregateId: asProjectId("project-fork"),
      type: "project.created",
      occurredAt: now,
      commandId: CommandId.make("cmd-project-create"),
      causationEventId: null,
      correlationId: CommandId.make("cmd-project-create"),
      metadata: {},
      payload: {
        projectId: asProjectId("project-fork"),
        title: "Project Fork",
        workspaceRoot: "/tmp/project-fork",
        defaultModelSelection: null,
        scripts: [],
        createdAt: now,
        updatedAt: now,
      },
    });

    return yield* projectEvent(withProject, {
      sequence: 2,
      eventId: asEventId("evt-thread-create-source"),
      aggregateKind: "thread",
      aggregateId: asThreadId("thread-source"),
      type: "thread.created",
      occurredAt: now,
      commandId: CommandId.make("cmd-thread-create-source"),
      causationEventId: null,
      correlationId: CommandId.make("cmd-thread-create-source"),
      metadata: {},
      payload: {
        threadId: asThreadId("thread-source"),
        projectId: asProjectId("project-fork"),
        title: "Source Thread",
        modelSelection: {
          instanceId: ProviderInstanceId.make("codex"),
          model: "gpt-5-codex",
        },
        interactionMode: DEFAULT_PROVIDER_INTERACTION_MODE,
        runtimeMode: "approval-required",
        branch: "main",
        worktreePath: "/tmp/project-fork/worktree",
        createdAt: now,
        updatedAt: now,
      },
    });
  });

function seedSourceMessages(
  readModel: OrchestrationReadModel,
): Effect.Effect<OrchestrationReadModel, OrchestrationProjectorDecodeError> {
  return projectEvent(readModel, {
    sequence: 3,
    eventId: asEventId("evt-message-sent"),
    aggregateKind: "thread",
    aggregateId: asThreadId("thread-source"),
    type: "thread.message-sent",
    occurredAt: now,
    commandId: CommandId.make("cmd-message-sent"),
    causationEventId: null,
    correlationId: CommandId.make("cmd-message-sent"),
    metadata: {},
    payload: {
      threadId: asThreadId("thread-source"),
      messageId: asMessageId("message-1"),
      role: "user",
      text: "First message",
      turnId: null,
      streaming: false,
      createdAt: now,
      updatedAt: now,
    },
  });
}

type PlannedEvent = Omit<OrchestrationEvent, "sequence">;

function singleEvent(result: PlannedEvent | ReadonlyArray<PlannedEvent>): PlannedEvent {
  const events = Array.isArray(result) ? result : [result];
  if (events.length !== 1) {
    throw new Error("Expected a single planned event, got an array.");
  }
  return events[0] as PlannedEvent;
}

it.layer(NodeServices.layer)("decider thread fork flows", (it) => {
  it.effect("thread.fork always fails and points at ThreadForkService", () =>
    Effect.gen(function* () {
      const readModel = yield* seedReadModel;
      const error = expectCommandInvariantError(
        yield* Effect.flip(
          decideOrchestrationCommand({
            command: {
              type: "thread.fork",
              commandId: CommandId.make("cmd-thread-fork"),
              sourceThreadId: asThreadId("thread-source"),
              newThreadId: asThreadId("thread-new"),
              mode: "full-history",
              createdAt: now,
            },
            readModel,
          }),
        ),
      );
      expect(error.commandType).toBe("thread.fork");
      expect(error.detail).toContain("ThreadForkService");
    }),
  );

  it.effect(
    "thread.fork.finalize full-history with no cutoff forks the entire message history",
    () =>
      Effect.gen(function* () {
        const readModel = yield* Effect.flatMap(seedReadModel, seedSourceMessages);

        const result = yield* decideOrchestrationCommand({
          command: {
            type: "thread.fork.finalize",
            commandId: CommandId.make("cmd-fork-finalize-full"),
            sourceThreadId: asThreadId("thread-source"),
            newThreadId: asThreadId("thread-new-full"),
            upToMessageId: null,
            mode: "full-history",
            pendingForkContextText: "context text",
            createdAt: now,
          },
          readModel,
        });

        const event = singleEvent(result);
        expect(event.type).toBe("thread.forked");
        const payload = event.payload as Record<string, unknown>;
        expect(payload).toMatchObject({
          threadId: asThreadId("thread-new-full"),
          projectId: asProjectId("project-fork"),
          title: "Source Thread (fork)",
          modelSelection: {
            instanceId: ProviderInstanceId.make("codex"),
            model: "gpt-5-codex",
          },
          runtimeMode: "approval-required",
          interactionMode: DEFAULT_PROVIDER_INTERACTION_MODE,
          branch: "main",
          worktreePath: "/tmp/project-fork/worktree",
          forkedFromThreadId: asThreadId("thread-source"),
          forkedUpToMessageId: null,
          forkMode: "full-history",
          pendingForkContextText: "context text",
          createdAt: now,
          updatedAt: now,
        });
        expect(payload.messages).toEqual([
          {
            id: asMessageId("message-1"),
            role: "user",
            text: "First message",
            turnId: null,
            streaming: false,
            createdAt: now,
            updatedAt: now,
          },
        ]);
      }),
  );

  it.effect("thread.fork.finalize full-history with a cutoff slices inclusively", () =>
    Effect.gen(function* () {
      const withProjectAndSource = yield* seedReadModel;
      const withFirstMessage = yield* seedSourceMessages(withProjectAndSource);
      const readModel = yield* projectEvent(withFirstMessage, {
        sequence: 4,
        eventId: asEventId("evt-message-sent-2"),
        aggregateKind: "thread",
        aggregateId: asThreadId("thread-source"),
        type: "thread.message-sent",
        occurredAt: now,
        commandId: CommandId.make("cmd-message-sent-2"),
        causationEventId: null,
        correlationId: CommandId.make("cmd-message-sent-2"),
        metadata: {},
        payload: {
          threadId: asThreadId("thread-source"),
          messageId: asMessageId("message-2"),
          role: "assistant",
          text: "Second message",
          turnId: null,
          streaming: false,
          createdAt: now,
          updatedAt: now,
        },
      });

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "thread.fork.finalize",
          commandId: CommandId.make("cmd-fork-finalize-cutoff"),
          sourceThreadId: asThreadId("thread-source"),
          newThreadId: asThreadId("thread-new-cutoff"),
          upToMessageId: asMessageId("message-1"),
          mode: "full-history",
          pendingForkContextText: null,
          createdAt: now,
        },
        readModel,
      });

      const event = singleEvent(result);
      const payload = event.payload as Record<string, unknown>;
      expect(payload.messages).toEqual([
        {
          id: asMessageId("message-1"),
          role: "user",
          text: "First message",
          turnId: null,
          streaming: false,
          createdAt: now,
          updatedAt: now,
        },
      ]);
      expect(payload.forkedUpToMessageId).toBe(asMessageId("message-1"));
    }),
  );

  it.effect(
    "thread.fork.finalize full-history fails when cutoff message does not exist on source thread",
    () =>
      Effect.gen(function* () {
        const readModel = yield* Effect.flatMap(seedReadModel, seedSourceMessages);

        const error = expectCommandInvariantError(
          yield* Effect.flip(
            decideOrchestrationCommand({
              command: {
                type: "thread.fork.finalize",
                commandId: CommandId.make("cmd-fork-finalize-missing-cutoff"),
                sourceThreadId: asThreadId("thread-source"),
                newThreadId: asThreadId("thread-new-missing-cutoff"),
                upToMessageId: asMessageId("message-does-not-exist"),
                mode: "full-history",
                pendingForkContextText: null,
                createdAt: now,
              },
              readModel,
            }),
          ),
        );

        expect(error.commandType).toBe("thread.fork.finalize");
        expect(error.detail).toContain(
          "Message 'message-does-not-exist' does not exist on source thread 'thread-source'.",
        );
      }),
  );

  it.effect("thread.fork.finalize summary mode builds a single synthetic assistant message", () =>
    Effect.gen(function* () {
      const readModel = yield* Effect.flatMap(seedReadModel, seedSourceMessages);

      const result = yield* decideOrchestrationCommand({
        command: {
          type: "thread.fork.finalize",
          commandId: CommandId.make("cmd-fork-finalize-summary"),
          sourceThreadId: asThreadId("thread-source"),
          newThreadId: asThreadId("thread-new-summary"),
          upToMessageId: null,
          mode: "summary",
          summaryText: "You were working on X.",
          pendingForkContextText: "You were working on X.",
          createdAt: now,
        },
        readModel,
      });

      const event = singleEvent(result);
      const payload = event.payload as Record<string, unknown>;
      expect(payload.forkMode).toBe("summary");
      expect(payload.pendingForkContextText).toBe("You were working on X.");
      expect(payload.messages).toHaveLength(1);
      const [message] = payload.messages as ReadonlyArray<Record<string, unknown>>;
      expect(message).toMatchObject({
        role: "assistant",
        text: "You were working on X.",
        turnId: null,
        streaming: false,
        createdAt: now,
        updatedAt: now,
      });
      expect(typeof message?.id).toBe("string");
    }),
  );

  it.effect("thread.fork.finalize summary mode without summaryText fails invariant", () =>
    Effect.gen(function* () {
      const readModel = yield* Effect.flatMap(seedReadModel, seedSourceMessages);

      const error = expectCommandInvariantError(
        yield* Effect.flip(
          decideOrchestrationCommand({
            command: {
              type: "thread.fork.finalize",
              commandId: CommandId.make("cmd-fork-finalize-summary-missing"),
              sourceThreadId: asThreadId("thread-source"),
              newThreadId: asThreadId("thread-new-summary-missing"),
              upToMessageId: null,
              mode: "summary",
              pendingForkContextText: null,
              createdAt: now,
            },
            readModel,
          }),
        ),
      );

      expect(error.commandType).toBe("thread.fork.finalize");
      expect(error.detail).toBe("summaryText is required to finalize a summary-mode thread fork.");
    }),
  );

  it.effect("thread.fork.finalize fails when source thread does not exist", () =>
    Effect.gen(function* () {
      const readModel = yield* seedReadModel;

      const error = expectCommandInvariantError(
        yield* Effect.flip(
          decideOrchestrationCommand({
            command: {
              type: "thread.fork.finalize",
              commandId: CommandId.make("cmd-fork-finalize-missing-source"),
              sourceThreadId: asThreadId("thread-does-not-exist"),
              newThreadId: asThreadId("thread-new-missing-source"),
              upToMessageId: null,
              mode: "full-history",
              pendingForkContextText: null,
              createdAt: now,
            },
            readModel,
          }),
        ),
      );

      expect(error.commandType).toBe("thread.fork.finalize");
      expect(error.detail).toBe(
        "Thread 'thread-does-not-exist' does not exist for command 'thread.fork.finalize'.",
      );
    }),
  );

  it.effect("thread.fork.finalize fails when newThreadId already exists", () =>
    Effect.gen(function* () {
      const readModel = yield* seedReadModel;

      const error = expectCommandInvariantError(
        yield* Effect.flip(
          decideOrchestrationCommand({
            command: {
              type: "thread.fork.finalize",
              commandId: CommandId.make("cmd-fork-finalize-existing-new"),
              sourceThreadId: asThreadId("thread-source"),
              newThreadId: asThreadId("thread-source"),
              upToMessageId: null,
              mode: "full-history",
              pendingForkContextText: null,
              createdAt: now,
            },
            readModel,
          }),
        ),
      );

      expect(error.commandType).toBe("thread.fork.finalize");
      expect(error.detail).toBe(
        "Thread 'thread-source' already exists and cannot be created twice.",
      );
    }),
  );
});
