import { assert, it } from "@effect/vitest";
import {
  EnvironmentId,
  type OrchestrationEvent,
  type OrchestrationProjectShell,
  type OrchestrationThread,
  type OrchestrationThreadShell,
  ProjectId,
  ProviderInstanceId,
  SubagentThreadError,
  ThreadId,
} from "@t3tools/contracts";
import { isTemporaryWorktreeBranch } from "@t3tools/shared/git";
import * as NodeServices from "@effect/platform-node/NodeServices";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as PubSub from "effect/PubSub";
import * as Ref from "effect/Ref";
import * as Stream from "effect/Stream";

import * as GitWorkflowService from "../../../git/GitWorkflowService.ts";
import * as OrchestrationEngine from "../../../orchestration/Services/OrchestrationEngine.ts";
import * as ProjectionSnapshotQuery from "../../../orchestration/Services/ProjectionSnapshotQuery.ts";
import * as ThreadBootstrap from "../../../orchestration/Services/ThreadBootstrap.ts";
import * as ProviderAdapterRegistry from "../../../provider/Services/ProviderAdapterRegistry.ts";
import * as McpInvocationContext from "../../McpInvocationContext.ts";
import { makeThreadsToolkitHandlers } from "./handlers.ts";

const PARENT_THREAD_ID = ThreadId.make("00000000-0000-4000-8000-00000000aaaa");
const CHILD_THREAD_ID = ThreadId.make("00000000-0000-4000-8000-00000000bbbb");
const PROJECT_ID = ProjectId.make("00000000-0000-4000-8000-00000000cccc");
const CLAUDE_INSTANCE = ProviderInstanceId.make("claudeAgent");
const NOW = "2026-01-01T00:00:00.000Z";

const invocationLayer = Layer.succeed(McpInvocationContext.McpInvocationContext, {
  environmentId: EnvironmentId.make("env-1"),
  threadId: PARENT_THREAD_ID,
  providerSessionId: "session-1",
  providerInstanceId: CLAUDE_INSTANCE,
  capabilities: new Set<McpInvocationContext.McpCapability>(["threads"]),
  issuedAt: 0,
  expiresAt: Number.MAX_SAFE_INTEGER,
});

const makeThreadShell = (
  overrides: Partial<OrchestrationThreadShell> = {},
): OrchestrationThreadShell => ({
  id: PARENT_THREAD_ID,
  projectId: PROJECT_ID,
  title: "Parent thread",
  modelSelection: { instanceId: CLAUDE_INSTANCE, model: "claude-opus-4-6" },
  runtimeMode: "full-access",
  interactionMode: "default",
  branch: null,
  worktreePath: null,
  parentThreadId: null,
  latestTurn: null,
  createdAt: NOW,
  updatedAt: NOW,
  archivedAt: null,
  session: null,
  latestUserMessageAt: null,
  hasPendingApprovals: false,
  hasPendingUserInput: false,
  hasActionableProposedPlan: false,
  ...overrides,
});

const projectShell: OrchestrationProjectShell = {
  id: PROJECT_ID,
  title: "Project",
  workspaceRoot: "/workspace/project",
  defaultModelSelection: null,
  scripts: [],
  createdAt: NOW,
  updatedAt: NOW,
};

interface HarnessOptions {
  readonly parentShell?: OrchestrationThreadShell;
  readonly childShell?: OrchestrationThreadShell | undefined;
  readonly snapshotThreads?: ReadonlyArray<OrchestrationThreadShell>;
  readonly childDetail?: Effect.Effect<Option.Option<OrchestrationThread>>;
  readonly domainEvents?: Stream.Stream<OrchestrationEvent>;
}

const makeHarness = (options: HarnessOptions = {}) =>
  Effect.gen(function* () {
    const dispatched = yield* Ref.make<ReadonlyArray<unknown>>([]);
    const parentShell = options.parentShell ?? makeThreadShell();
    const childShell = options.childShell;

    const projectionLayer = Layer.mock(ProjectionSnapshotQuery.ProjectionSnapshotQuery)({
      getThreadShellById: (threadId) =>
        Effect.succeed(
          threadId === parentShell.id
            ? Option.some(parentShell)
            : threadId === childShell?.id
              ? Option.some(childShell)
              : Option.none(),
        ),
      getShellSnapshot: () =>
        Effect.succeed({
          snapshotSequence: 1,
          projects: [projectShell],
          threads: [parentShell, ...(options.snapshotThreads ?? [])],
          updatedAt: NOW,
        }),
      getProjectShellById: () => Effect.succeed(Option.some(projectShell)),
      getThreadDetailById: () => options.childDetail ?? Effect.succeed(Option.none()),
    });
    const registryLayer = Layer.mock(ProviderAdapterRegistry.ProviderAdapterRegistry)({
      getInstanceInfo: (instanceId) =>
        Effect.succeed({
          instanceId,
          driverKind: "claudeAgent",
          displayName: undefined,
          enabled: true,
          continuationIdentity: "session-id",
        } as never),
    });
    const gitLayer = Layer.mock(GitWorkflowService.GitWorkflowService)({
      localStatus: () =>
        Effect.succeed({
          isRepo: true,
          hasPrimaryRemote: false,
          isDefaultRef: true,
          refName: "main",
          hasWorkingTreeChanges: false,
          workingTree: { files: [] },
        } as never),
    });
    const bootstrapLayer = Layer.mock(ThreadBootstrap.ThreadBootstrapService)({
      dispatchBootstrapTurnStart: (command) =>
        Ref.update(dispatched, (commands) => [...commands, command]).pipe(
          Effect.as({ sequence: 1 }),
        ),
    });
    const engineLayer = Layer.mock(OrchestrationEngine.OrchestrationEngineService)({
      streamDomainEvents: options.domainEvents ?? Stream.empty,
    });

    return {
      dispatched,
      layer: Layer.mergeAll(
        invocationLayer,
        projectionLayer,
        registryLayer,
        gitLayer,
        bootstrapLayer,
        engineLayer,
      ),
    };
  });

const spawnInput = { prompt: "Summarize the repo layout" };

it.layer(NodeServices.layer)("threads toolkit handlers", (it) => {
  it.effect("spawns a worktree-isolated child linked to the parent", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness();
      const result = yield* (yield* makeThreadsToolkitHandlers)
        .spawn_thread(spawnInput)
        .pipe(Effect.provide(harness.layer));

      assert.strictEqual(result.title, "Summarize the repo layout");
      const commands = yield* Ref.get(harness.dispatched);
      assert.strictEqual(commands.length, 1);
      const command = commands[0] as {
        readonly threadId: string;
        readonly message: { readonly text: string };
        readonly runtimeMode: string;
        readonly bootstrap: {
          readonly createThread: {
            readonly parentThreadId: string;
            readonly modelSelection: { readonly instanceId: string; readonly model: string };
          };
          readonly prepareWorktree?: {
            readonly projectCwd: string;
            readonly baseBranch: string;
            readonly branch: string;
          };
          readonly runSetupScript?: boolean;
        };
      };
      assert.strictEqual(command.threadId, result.threadId);
      assert.strictEqual(command.message.text, spawnInput.prompt);
      assert.strictEqual(command.runtimeMode, "full-access");
      assert.strictEqual(command.bootstrap.createThread.parentThreadId, PARENT_THREAD_ID);
      assert.deepStrictEqual(command.bootstrap.createThread.modelSelection, {
        instanceId: CLAUDE_INSTANCE,
        model: "claude-opus-4-6",
      });
      assert.strictEqual(command.bootstrap.prepareWorktree?.projectCwd, projectShell.workspaceRoot);
      assert.strictEqual(command.bootstrap.prepareWorktree?.baseBranch, "main");
      assert.isTrue(isTemporaryWorktreeBranch(command.bootstrap.prepareWorktree?.branch ?? ""));
      assert.strictEqual(command.bootstrap.runSetupScript, true);
    }),
  );

  it.effect("gives each spawned child its own worktree branch", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness();
      const handlers = yield* makeThreadsToolkitHandlers;
      yield* handlers.spawn_thread(spawnInput).pipe(Effect.provide(harness.layer));
      yield* handlers.spawn_thread(spawnInput).pipe(Effect.provide(harness.layer));

      const commands = yield* Ref.get(harness.dispatched);
      const branches = commands.map(
        (command) =>
          (command as { readonly bootstrap: { readonly prepareWorktree?: { branch: string } } })
            .bootstrap.prepareWorktree?.branch,
      );
      assert.strictEqual(branches.length, 2);
      assert.isDefined(branches[0]);
      assert.notStrictEqual(branches[0], branches[1]);
    }),
  );

  it.effect("shares the parent checkout when envMode is local", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness({
        parentShell: makeThreadShell({ branch: "feature", worktreePath: "/worktrees/feature" }),
      });
      yield* (yield* makeThreadsToolkitHandlers)
        .spawn_thread({ ...spawnInput, envMode: "local" })
        .pipe(Effect.provide(harness.layer));

      const commands = yield* Ref.get(harness.dispatched);
      const command = commands[0] as {
        readonly bootstrap: {
          readonly createThread: { readonly branch: string; readonly worktreePath: string };
          readonly prepareWorktree?: unknown;
        };
      };
      assert.strictEqual(command.bootstrap.createThread.branch, "feature");
      assert.strictEqual(command.bootstrap.createThread.worktreePath, "/worktrees/feature");
      assert.strictEqual(command.bootstrap.prepareWorktree, undefined);
    }),
  );

  it.effect("clamps a child runtime mode that escalates past the parent", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness({
        parentShell: makeThreadShell({ runtimeMode: "approval-required" }),
      });
      yield* (yield* makeThreadsToolkitHandlers)
        .spawn_thread({ ...spawnInput, runtimeMode: "full-access" })
        .pipe(Effect.provide(harness.layer));

      const commands = yield* Ref.get(harness.dispatched);
      const command = commands[0] as {
        readonly runtimeMode: string;
        readonly bootstrap: { readonly createThread: { readonly runtimeMode: string } };
      };
      assert.strictEqual(command.runtimeMode, "approval-required");
      assert.strictEqual(command.bootstrap.createThread.runtimeMode, "approval-required");
    }),
  );

  it.effect("rejects spawning from a spawned child", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness({
        parentShell: makeThreadShell({ parentThreadId: CHILD_THREAD_ID }),
      });
      const error = yield* (yield* makeThreadsToolkitHandlers)
        .spawn_thread(spawnInput)
        .pipe(Effect.provide(harness.layer), Effect.flip);
      assert.instanceOf(error, SubagentThreadError);
      assert.strictEqual(error.reason, "depth_exceeded");
    }),
  );

  it.effect("rejects spawning past the running-children limit", () =>
    Effect.gen(function* () {
      const runningChild = (suffix: string) =>
        makeThreadShell({
          id: ThreadId.make(`00000000-0000-4000-8000-0000000000${suffix}`),
          parentThreadId: PARENT_THREAD_ID,
          latestTurn: {
            turnId: "00000000-0000-4000-8000-00000000dddd",
            state: "running",
            requestedAt: NOW,
            startedAt: NOW,
            completedAt: null,
            assistantMessageId: null,
            pendingMessageId: null,
          } as never,
        });
      const harness = yield* makeHarness({
        snapshotThreads: [
          runningChild("01"),
          runningChild("02"),
          runningChild("03"),
          runningChild("04"),
        ],
      });
      const error = yield* (yield* makeThreadsToolkitHandlers)
        .spawn_thread(spawnInput)
        .pipe(Effect.provide(harness.layer), Effect.flip);
      assert.strictEqual(error.reason, "concurrency_exceeded");
    }),
  );

  it.effect("counts children pending their first session toward the limit", () =>
    Effect.gen(function* () {
      const pendingChild = (suffix: string) =>
        makeThreadShell({
          id: ThreadId.make(`00000000-0000-4000-8000-0000000000${suffix}`),
          parentThreadId: PARENT_THREAD_ID,
          latestTurn: null,
          session: null,
        });
      const harness = yield* makeHarness({
        snapshotThreads: [
          pendingChild("01"),
          pendingChild("02"),
          pendingChild("03"),
          pendingChild("04"),
        ],
      });
      const error = yield* (yield* makeThreadsToolkitHandlers)
        .spawn_thread(spawnInput)
        .pipe(Effect.provide(harness.layer), Effect.flip);
      assert.strictEqual(error.reason, "concurrency_exceeded");
    }),
  );

  it.effect("requires a model when the provider differs from the parent", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness();
      const error = yield* (yield* makeThreadsToolkitHandlers)
        .spawn_thread({ ...spawnInput, providerInstanceId: ProviderInstanceId.make("codex") })
        .pipe(Effect.provide(harness.layer), Effect.flip);
      assert.strictEqual(error.reason, "unknown_provider");
    }),
  );

  it.effect("rejects awaiting a thread that is not a child", () =>
    Effect.gen(function* () {
      const harness = yield* makeHarness({
        childShell: makeThreadShell({ id: CHILD_THREAD_ID, parentThreadId: null }),
      });
      const error = yield* (yield* makeThreadsToolkitHandlers)
        .await_thread({ threadId: CHILD_THREAD_ID })
        .pipe(Effect.provide(harness.layer), Effect.flip);
      assert.strictEqual(error.reason, "not_a_child");
    }),
  );

  it.effect("returns the final message for a settled child", () =>
    Effect.gen(function* () {
      const detail = {
        latestTurn: {
          state: "completed",
          assistantMessageId: "message-1",
        },
        messages: [{ id: "message-1", role: "assistant", text: "All done." }],
      } as unknown as OrchestrationThread;
      const harness = yield* makeHarness({
        childShell: makeThreadShell({ id: CHILD_THREAD_ID, parentThreadId: PARENT_THREAD_ID }),
        childDetail: Effect.succeed(Option.some(detail)),
      });
      const result = yield* (yield* makeThreadsToolkitHandlers)
        .await_thread({ threadId: CHILD_THREAD_ID })
        .pipe(Effect.provide(harness.layer));
      assert.strictEqual(result.status, "completed");
      assert.strictEqual(result.finalMessage, "All done.");
    }),
  );

  it.effect("times out while a child keeps running", () =>
    Effect.gen(function* () {
      const runningDetail = {
        latestTurn: { state: "running", assistantMessageId: null },
        messages: [],
      } as unknown as OrchestrationThread;
      const harness = yield* makeHarness({
        childShell: makeThreadShell({ id: CHILD_THREAD_ID, parentThreadId: PARENT_THREAD_ID }),
        childDetail: Effect.succeed(Option.some(runningDetail)),
      });
      const result = yield* (yield* makeThreadsToolkitHandlers)
        .await_thread({ threadId: CHILD_THREAD_ID, timeoutSeconds: 0 })
        .pipe(Effect.provide(harness.layer));
      assert.strictEqual(result.status, "timeout");
      assert.strictEqual(result.finalMessage, null);
    }),
  );

  it.effect("resolves once a domain event settles the child", () =>
    Effect.gen(function* () {
      const detailState = yield* Ref.make<Option.Option<OrchestrationThread>>(
        Option.some({
          latestTurn: { state: "running", assistantMessageId: null },
          messages: [],
        } as unknown as OrchestrationThread),
      );
      const events = yield* PubSub.unbounded<OrchestrationEvent>();
      const harness = yield* makeHarness({
        childShell: makeThreadShell({ id: CHILD_THREAD_ID, parentThreadId: PARENT_THREAD_ID }),
        childDetail: Ref.get(detailState),
        domainEvents: Stream.fromPubSub(events),
      });

      const handlersForFork = yield* makeThreadsToolkitHandlers;
      const awaiting = yield* handlersForFork
        .await_thread({ threadId: CHILD_THREAD_ID, timeoutSeconds: 30 })
        .pipe(Effect.provide(harness.layer), Effect.forkChild);

      yield* Ref.set(
        detailState,
        Option.some({
          latestTurn: { state: "completed", assistantMessageId: "message-2" },
          messages: [{ id: "message-2", role: "assistant", text: "Finished." }],
        } as unknown as OrchestrationThread),
      );
      yield* PubSub.publish(events, {
        type: "thread.session-set",
        aggregateId: CHILD_THREAD_ID,
      } as never);

      const result = yield* Fiber.join(awaiting);
      assert.strictEqual(result.status, "completed");
      assert.strictEqual(result.finalMessage, "Finished.");
    }),
  );
});
