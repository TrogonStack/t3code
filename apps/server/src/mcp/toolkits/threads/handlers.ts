import {
  type AwaitThreadInput,
  type AwaitThreadResult,
  CommandId,
  DEFAULT_PROVIDER_INTERACTION_MODE,
  MessageId,
  SUBAGENT_AWAIT_DEFAULT_TIMEOUT_SECONDS,
  SUBAGENT_MAX_DEPTH,
  SUBAGENT_MAX_RUNNING_PER_TREE,
  type RuntimeMode,
  type SpawnThreadInput,
  SubagentThreadError,
  ThreadId,
} from "@t3tools/contracts";
import { buildTemporaryWorktreeBranchName } from "@t3tools/shared/git";
import * as Crypto from "effect/Crypto";
import * as DateTime from "effect/DateTime";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Semaphore from "effect/Semaphore";
import * as Stream from "effect/Stream";

import * as GitWorkflowService from "../../../git/GitWorkflowService.ts";
import * as OrchestrationEngine from "../../../orchestration/Services/OrchestrationEngine.ts";
import * as ProjectionSnapshotQuery from "../../../orchestration/Services/ProjectionSnapshotQuery.ts";
import * as ThreadBootstrap from "../../../orchestration/Services/ThreadBootstrap.ts";
import * as ProviderAdapterRegistry from "../../../provider/Services/ProviderAdapterRegistry.ts";
import * as McpInvocationContext from "../../McpInvocationContext.ts";
import { ThreadsToolkit } from "./tools.ts";

const SUBAGENT_TITLE_MAX_CHARS = 60;
const SUBAGENT_AWAIT_POLL_INTERVAL = Duration.seconds(5);
const SUBAGENT_PENDING_GRACE_MS = Duration.toMillis(Duration.minutes(5));

const subagentError = (
  reason: SubagentThreadError["reason"],
  detail: string,
): SubagentThreadError => new SubagentThreadError({ reason, detail });

const requireThreadsScope = McpInvocationContext.requireMcpCapability("threads").pipe(
  Effect.mapError(() =>
    subagentError("not_permitted", "Subagent threads are not available for this session."),
  ),
);

const deriveSubagentTitle = (prompt: string): string => {
  const firstLine = prompt.split("\n", 1)[0]?.trim() ?? "";
  if (firstLine.length === 0) {
    return "Subagent task";
  }
  return firstLine.length > SUBAGENT_TITLE_MAX_CHARS
    ? `${firstLine.slice(0, SUBAGENT_TITLE_MAX_CHARS - 1)}…`
    : firstLine;
};

const RUNTIME_MODE_PERMISSIVENESS: Record<RuntimeMode, number> = {
  "approval-required": 0,
  "auto-accept-edits": 1,
  "full-access": 2,
};

// A child may not run more permissively than the thread that spawned it.
const clampRuntimeMode = (requested: RuntimeMode | undefined, parent: RuntimeMode): RuntimeMode =>
  requested !== undefined &&
  RUNTIME_MODE_PERMISSIVENESS[requested] <= RUNTIME_MODE_PERMISSIVENESS[parent]
    ? requested
    : parent;

const readSettledResult = (
  threadId: ThreadId,
): Effect.Effect<
  AwaitThreadResult | undefined,
  SubagentThreadError,
  ProjectionSnapshotQuery.ProjectionSnapshotQuery
> =>
  Effect.gen(function* () {
    const projectionSnapshotQuery = yield* ProjectionSnapshotQuery.ProjectionSnapshotQuery;
    const thread = yield* projectionSnapshotQuery
      .getThreadDetailById(threadId)
      .pipe(Effect.mapError((error) => subagentError("spawn_failed", error.message)));
    if (Option.isNone(thread)) {
      return {
        threadId,
        status: "failed",
        finalMessage: null,
      } satisfies AwaitThreadResult;
    }
    const latestTurn = thread.value.latestTurn;
    if (latestTurn === null) {
      // A terminal session with no projected turn means the child died
      // during bootstrap; report that instead of polling until timeout.
      const sessionStatus = thread.value.session?.status;
      if (sessionStatus === "interrupted") {
        return { threadId, status: "interrupted", finalMessage: null } satisfies AwaitThreadResult;
      }
      if (sessionStatus === "stopped" || sessionStatus === "error") {
        return { threadId, status: "failed", finalMessage: null } satisfies AwaitThreadResult;
      }
      return undefined;
    }
    if (latestTurn.state === "running") {
      return undefined;
    }
    const finalMessage =
      latestTurn.assistantMessageId === null
        ? null
        : (thread.value.messages.find((message) => message.id === latestTurn.assistantMessageId)
            ?.text ?? null);
    return {
      threadId,
      status:
        latestTurn.state === "completed"
          ? ("completed" as const)
          : latestTurn.state === "interrupted"
            ? ("interrupted" as const)
            : ("failed" as const),
      finalMessage,
    } satisfies AwaitThreadResult;
  });

export const makeThreadsToolkitHandlers = Effect.gen(function* () {
  const spawnAdmission = yield* Semaphore.make(1);
  return {
    spawn_thread: (input: SpawnThreadInput) =>
      // Admission (limit check) and dispatch run under one permit so
      // concurrent spawns cannot both pass the running-children gate.
      spawnAdmission.withPermits(1)(
        Effect.gen(function* () {
          const scope = yield* requireThreadsScope;
          const projectionSnapshotQuery = yield* ProjectionSnapshotQuery.ProjectionSnapshotQuery;
          const providerAdapterRegistry = yield* ProviderAdapterRegistry.ProviderAdapterRegistry;
          const gitWorkflow = yield* GitWorkflowService.GitWorkflowService;
          const threadBootstrap = yield* ThreadBootstrap.ThreadBootstrapService;
          const crypto = yield* Crypto.Crypto;

          const parentShell = yield* projectionSnapshotQuery
            .getThreadShellById(scope.threadId)
            .pipe(
              Effect.mapError((error) => subagentError("spawn_failed", error.message)),
              Effect.flatMap(
                Option.match({
                  onNone: () =>
                    Effect.fail(
                      subagentError("unknown_thread", "The calling thread no longer exists."),
                    ),
                  onSome: Effect.succeed,
                }),
              ),
            );
          const shellSnapshot = yield* projectionSnapshotQuery
            .getShellSnapshot()
            .pipe(Effect.mapError((error) => subagentError("spawn_failed", error.message)));
          const shellById = new Map(shellSnapshot.threads.map((thread) => [thread.id, thread]));

          // Walk the ancestry to bound tree depth and find the tree root; the
          // running-subagent budget is shared by the whole tree, not per
          // spawning thread.
          let callerDepth = 0;
          let rootThreadId = scope.threadId;
          {
            const visited = new Set<string>([parentShell.id]);
            let current = parentShell;
            while ((current.parentThreadId ?? null) !== null) {
              const ancestor = shellById.get(current.parentThreadId as ThreadId);
              if (ancestor === undefined || visited.has(ancestor.id)) {
                break;
              }
              visited.add(ancestor.id);
              callerDepth += 1;
              rootThreadId = ancestor.id;
              current = ancestor;
            }
          }
          if (callerDepth + 1 > SUBAGENT_MAX_DEPTH) {
            return yield* subagentError(
              "depth_exceeded",
              `Subagent trees can nest at most ${SUBAGENT_MAX_DEPTH} levels.`,
            );
          }

          // A freshly spawned child has no latest turn until its provider
          // session reports in, so pending children must count against the
          // cap alongside running ones. The pending state is time-bounded:
          // a child that dies before its session ever reports in must not
          // occupy a slot forever.
          const nowMillis = DateTime.toEpochMillis(yield* DateTime.now);
          const occupiesSlot = (thread: (typeof shellSnapshot.threads)[number]): boolean => {
            if (thread.latestTurn !== null) {
              return thread.latestTurn.state === "running";
            }
            const sessionStatus = thread.session?.status;
            if (sessionStatus === "starting" || sessionStatus === "running") {
              return true;
            }
            if (
              sessionStatus === "stopped" ||
              sessionStatus === "error" ||
              sessionStatus === "interrupted"
            ) {
              return false;
            }
            return nowMillis - Date.parse(thread.createdAt) < SUBAGENT_PENDING_GRACE_MS;
          };
          const childrenByParent = new Map<string, Array<(typeof shellSnapshot.threads)[number]>>();
          for (const thread of shellSnapshot.threads) {
            const threadParentId = thread.parentThreadId ?? null;
            if (threadParentId !== null) {
              const siblings = childrenByParent.get(threadParentId) ?? [];
              siblings.push(thread);
              childrenByParent.set(threadParentId, siblings);
            }
          }
          let runningDescendants = 0;
          {
            const visited = new Set<string>();
            const stack = [rootThreadId as string];
            while (stack.length > 0) {
              const currentId = stack.pop();
              if (currentId === undefined || visited.has(currentId)) {
                continue;
              }
              visited.add(currentId);
              for (const child of childrenByParent.get(currentId) ?? []) {
                if (occupiesSlot(child)) {
                  runningDescendants += 1;
                }
                stack.push(child.id);
              }
            }
          }
          if (runningDescendants >= SUBAGENT_MAX_RUNNING_PER_TREE) {
            return yield* subagentError(
              "concurrency_exceeded",
              `At most ${SUBAGENT_MAX_RUNNING_PER_TREE} subagents may run at once across a tree. Await some with await_thread before spawning more.`,
            );
          }

          const instanceId = input.providerInstanceId ?? parentShell.modelSelection.instanceId;
          const instanceInfo = yield* providerAdapterRegistry
            .getInstanceInfo(instanceId)
            .pipe(
              Effect.mapError(() =>
                subagentError(
                  "unknown_provider",
                  `Provider instance '${instanceId}' is not available.`,
                ),
              ),
            );
          if (!instanceInfo.enabled) {
            return yield* subagentError(
              "unknown_provider",
              `Provider instance '${instanceId}' is disabled.`,
            );
          }
          const model =
            input.model ??
            (instanceId === parentShell.modelSelection.instanceId
              ? parentShell.modelSelection.model
              : undefined);
          if (model === undefined) {
            return yield* subagentError(
              "unknown_provider",
              "model is required when providerInstanceId differs from the calling thread's provider.",
            );
          }

          const project = yield* projectionSnapshotQuery
            .getProjectShellById(parentShell.projectId)
            .pipe(
              Effect.mapError((error) => subagentError("spawn_failed", error.message)),
              Effect.flatMap(
                Option.match({
                  onNone: () =>
                    Effect.fail(
                      subagentError(
                        "spawn_failed",
                        "The calling thread's project no longer exists.",
                      ),
                    ),
                  onSome: Effect.succeed,
                }),
              ),
            );

          const envMode = input.envMode ?? "worktree";
          const runtimeMode = clampRuntimeMode(input.runtimeMode, parentShell.runtimeMode);
          const title = input.title ?? deriveSubagentTitle(input.prompt);
          const threadId = ThreadId.make(yield* crypto.randomUUIDv4.pipe(Effect.orDie));
          const createdAt = DateTime.formatIso(yield* DateTime.now);
          const commandId = CommandId.make(yield* crypto.randomUUIDv4.pipe(Effect.orDie));
          const messageId = MessageId.make(yield* crypto.randomUUIDv4.pipe(Effect.orDie));

          let prepareWorktree:
            | { readonly projectCwd: string; readonly baseBranch: string; readonly branch: string }
            | undefined;
          if (envMode === "worktree") {
            const baseBranch =
              parentShell.branch ??
              (yield* gitWorkflow.localStatus({ cwd: project.workspaceRoot }).pipe(
                Effect.mapError((error) => subagentError("spawn_failed", error.message)),
                Effect.map((status) => status.refName),
              ));
            if (baseBranch === null) {
              return yield* subagentError(
                "spawn_failed",
                'Cannot prepare a worktree: the project has no current branch. Retry with envMode "local".',
              );
            }
            const branchHex = (yield* crypto.randomUUIDv4.pipe(Effect.orDie)).replaceAll("-", "");
            prepareWorktree = {
              projectCwd: project.workspaceRoot,
              baseBranch,
              branch: buildTemporaryWorktreeBranchName((byteLength) =>
                branchHex.slice(0, byteLength * 2),
              ),
            };
          }

          yield* threadBootstrap
            .dispatchBootstrapTurnStart({
              type: "thread.turn.start",
              commandId,
              threadId,
              message: {
                messageId,
                role: "user",
                text: input.prompt,
                attachments: [],
              },
              runtimeMode,
              interactionMode: DEFAULT_PROVIDER_INTERACTION_MODE,
              bootstrap: {
                createThread: {
                  projectId: parentShell.projectId,
                  title,
                  modelSelection: { instanceId, model },
                  runtimeMode,
                  interactionMode: DEFAULT_PROVIDER_INTERACTION_MODE,
                  branch: envMode === "local" ? parentShell.branch : null,
                  worktreePath: envMode === "local" ? parentShell.worktreePath : null,
                  parentThreadId: scope.threadId,
                  createdAt,
                },
                ...(prepareWorktree ? { prepareWorktree, runSetupScript: true } : {}),
              },
              createdAt,
            })
            .pipe(Effect.mapError((error) => subagentError("spawn_failed", error.message)));

          return { threadId, title };
        }),
      ),

    await_thread: (input: AwaitThreadInput) =>
      Effect.gen(function* () {
        const scope = yield* requireThreadsScope;
        const projectionSnapshotQuery = yield* ProjectionSnapshotQuery.ProjectionSnapshotQuery;
        const orchestrationEngine = yield* OrchestrationEngine.OrchestrationEngineService;
        const childThreadId = ThreadId.make(input.threadId);

        const childShell = yield* projectionSnapshotQuery.getThreadShellById(childThreadId).pipe(
          Effect.mapError((error) => subagentError("spawn_failed", error.message)),
          Effect.flatMap(
            Option.match({
              onNone: () =>
                Effect.fail(subagentError("unknown_thread", "That thread does not exist.")),
              onSome: Effect.succeed,
            }),
          ),
        );
        if ((childShell.parentThreadId ?? null) !== scope.threadId) {
          return yield* subagentError(
            "not_a_child",
            "Only threads spawned by this thread can be awaited.",
          );
        }

        const timeoutSeconds = input.timeoutSeconds ?? SUBAGENT_AWAIT_DEFAULT_TIMEOUT_SECONDS;
        // Merge (not concat) so the domain-event subscription starts alongside
        // the initial check, and keep a slow poll as a backstop for settlement
        // that slips between reads.
        const firstSettled = Stream.merge(
          orchestrationEngine.streamDomainEvents.pipe(
            Stream.filter((event) => event.aggregateId === childThreadId),
            Stream.map(() => undefined),
          ),
          Stream.merge(
            Stream.make(undefined),
            Stream.tick(SUBAGENT_AWAIT_POLL_INTERVAL).pipe(Stream.map(() => undefined)),
          ),
        ).pipe(
          Stream.mapEffect(() => readSettledResult(childThreadId)),
          Stream.filter((result): result is AwaitThreadResult => result !== undefined),
          Stream.take(1),
          Stream.runCollect,
        );

        const settled = yield* firstSettled.pipe(
          Effect.timeoutOption(Duration.seconds(timeoutSeconds)),
        );
        return Option.match(settled, {
          onNone: () =>
            ({
              threadId: childThreadId,
              status: "timeout",
              finalMessage: null,
            }) satisfies AwaitThreadResult,
          onSome: (results) =>
            Array.from(results)[0] ??
            ({
              threadId: childThreadId,
              status: "timeout",
              finalMessage: null,
            } satisfies AwaitThreadResult),
        });
      }),
  } satisfies Parameters<typeof ThreadsToolkit.toLayer>[0];
});

export const ThreadsToolkitHandlersLive = ThreadsToolkit.toLayer(makeThreadsToolkitHandlers);
