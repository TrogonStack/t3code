import {
  OrchestrationDispatchCommandError,
  type MessageId,
  type OrchestrationMessage,
  type TurnId,
} from "@t3tools/contracts";
import * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import { resolveThreadWorkspaceCwd } from "../../checkpointing/Utils.ts";
import { ProviderService } from "../../provider/Services/ProviderService.ts";
import { ServerSettingsService } from "../../serverSettings.ts";
import { TextGeneration } from "../../textGeneration/TextGeneration.ts";
import { renderTranscript } from "../../textGeneration/TextGenerationPrompts.ts";
import { OrchestrationEngineService } from "../Services/OrchestrationEngine.ts";
import { ProjectionSnapshotQuery } from "../Services/ProjectionSnapshotQuery.ts";
import { ThreadForkService, type ThreadForkShape } from "../Services/ThreadFork.ts";

const isOrchestrationDispatchCommandError = Schema.is(OrchestrationDispatchCommandError);

const toDispatchCommandCauseError = (cause: Cause.Cause<unknown>) => {
  const error = Cause.squash(cause);
  return isOrchestrationDispatchCommandError(error)
    ? error
    : new OrchestrationDispatchCommandError({
        message: error instanceof Error ? error.message : "Failed to fork thread.",
        cause,
      });
};

interface MessageSlice {
  readonly messages: ReadonlyArray<OrchestrationMessage>;
  readonly upToTurnId: TurnId | null;
}

function sliceMessagesUpTo(
  messages: ReadonlyArray<OrchestrationMessage>,
  upToMessageId: MessageId | null,
): Effect.Effect<MessageSlice, OrchestrationDispatchCommandError> {
  if (upToMessageId === null) {
    return Effect.succeed({ messages, upToTurnId: null });
  }
  const cutoffMessage = messages.find((message) => message.id === upToMessageId);
  if (!cutoffMessage) {
    return Effect.fail(
      new OrchestrationDispatchCommandError({
        message: `Message '${upToMessageId}' does not exist on the source thread.`,
      }),
    );
  }
  const cutoffIndex = messages.indexOf(cutoffMessage);
  return Effect.succeed({
    messages: messages.slice(0, cutoffIndex + 1),
    upToTurnId: cutoffMessage.turnId,
  });
}

const makeThreadFork = Effect.gen(function* () {
  const orchestrationEngine = yield* OrchestrationEngineService;
  const projectionSnapshotQuery = yield* ProjectionSnapshotQuery;
  const providerService = yield* ProviderService;
  const textGeneration = yield* TextGeneration;
  const serverSettingsService = yield* ServerSettingsService;

  const dispatchFork: ThreadForkShape["dispatchFork"] = (command) =>
    Effect.gen(function* () {
      const upToMessageId = command.upToMessageId ?? null;

      const source = yield* projectionSnapshotQuery.getThreadDetailById(command.sourceThreadId);
      if (Option.isNone(source)) {
        return yield* new OrchestrationDispatchCommandError({
          message: `Source thread '${command.sourceThreadId}' was not found.`,
        });
      }
      const sourceThread = source.value;

      const { messages: slicedMessages, upToTurnId } = yield* sliceMessagesUpTo(
        sourceThread.messages,
        upToMessageId,
      );

      const project = yield* projectionSnapshotQuery
        .getProjectShellById(sourceThread.projectId)
        .pipe(Effect.map(Option.getOrUndefined));
      const cwd =
        resolveThreadWorkspaceCwd({
          thread: sourceThread,
          projects: project ? [project] : [],
        }) ?? process.cwd();

      let summaryText: string | undefined;
      let pendingForkContextText: string | null;

      if (command.mode === "summary") {
        const { textGenerationModelSelection: modelSelection } =
          yield* serverSettingsService.getSettings;
        const generated = yield* textGeneration.generateThreadForkSummary({
          cwd,
          messages: slicedMessages,
          modelSelection,
        });
        summaryText = generated.summary;
        pendingForkContextText = generated.summary;
      } else {
        const forkResult = yield* providerService.forkConversation({
          sourceThreadId: command.sourceThreadId,
          newThreadId: command.newThreadId,
          upToMessageId,
          upToTurnId,
        });
        pendingForkContextText = forkResult.supported ? null : renderTranscript(slicedMessages);
      }

      return yield* orchestrationEngine.dispatch({
        type: "thread.fork.finalize",
        commandId: command.commandId,
        sourceThreadId: command.sourceThreadId,
        newThreadId: command.newThreadId,
        upToMessageId,
        mode: command.mode,
        ...(command.title !== undefined ? { title: command.title } : {}),
        ...(summaryText !== undefined ? { summaryText } : {}),
        pendingForkContextText,
        createdAt: command.createdAt,
      });
    }).pipe(Effect.catchCause((cause) => Effect.fail(toDispatchCommandCauseError(cause))));

  return {
    dispatchFork,
  } satisfies ThreadForkShape;
});

export const ThreadForkLive = Layer.effect(ThreadForkService, makeThreadFork);
