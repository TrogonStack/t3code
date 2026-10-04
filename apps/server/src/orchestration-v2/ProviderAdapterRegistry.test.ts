import * as NodeServices from "@effect/platform-node/NodeServices";
import { assert, it } from "@effect/vitest";
import {
  ProviderDriverKind,
  ProviderInstanceEnvironment,
  ProviderInstanceId,
  ProviderSessionId,
  ThreadId,
  ProviderSetupError,
  type ProviderInstanceConfigMap,
} from "@t3tools/contracts";
import { HostProcessEnvironment } from "@t3tools/shared/hostProcess";
import * as Deferred from "effect/Deferred";
import * as Effect from "effect/Effect";
import * as Exit from "effect/Exit";
import * as Fiber from "effect/Fiber";
import * as Layer from "effect/Layer";
import * as Ref from "effect/Ref";
import * as Schema from "effect/Schema";
import type * as Scope from "effect/Scope";
import * as Stream from "effect/Stream";

import * as ProviderAuthFlow from "../provider/ProviderAuthFlow.ts";
import type { ProviderAuthController } from "../provider/Services/ProviderAuthService.ts";
import { makeProviderInstanceRegistry } from "../provider/Layers/ProviderInstanceRegistryLive.ts";
import type { ProviderDriver, ProviderInstance } from "../provider/ProviderDriver.ts";
import { mergeProviderInstanceEnvironment } from "../provider/ProviderInstanceEnvironment.ts";
import { hasProviderSecretReference } from "../provider/ProviderSecretReference.ts";
import * as ProviderInstanceRegistry from "../provider/Services/ProviderInstanceRegistry.ts";
import * as ProviderSecretResolver from "../provider/Services/ProviderSecretResolver.ts";
import { ProviderAdapterOpenSessionError, type ProviderAdapterV2Shape } from "./ProviderAdapter.ts";
import {
  ProviderAdapterDriverCreateError,
  type ProviderAdapterDriver,
} from "./ProviderAdapterDriver.ts";
import * as ProviderAdapterRegistry from "./ProviderAdapterRegistry.ts";

const driver = ProviderDriverKind.make("codex");
const personalId = ProviderInstanceId.make("codex_personal");
const workId = ProviderInstanceId.make("codex_work");

const HOME_ACCOUNT = "my.1password.com";
const decodeEnvironment = Schema.decodeSync(ProviderInstanceEnvironment);
const onePasswordVariable = (name: string, reference: string) => ({
  name,
  value: { kind: "1password" as const, reference, account: HOME_ACCOUNT },
});

const makeAdapter = (instanceId: ProviderInstanceId): ProviderAdapterV2Shape =>
  ({
    instanceId,
    driver,
    getCapabilities: () => Effect.die("capabilities are not used by this registry test"),
    planSelectionTransition: () => Effect.succeed({ type: "apply_on_next_turn" }),
    openSession: () => Effect.die("sessions are not used by this registry test"),
  }) as ProviderAdapterV2Shape;

const makeInstance = (
  instanceId: ProviderInstanceId,
  orchestrationAdapter: ProviderAdapterV2Shape,
): ProviderInstance => ({
  instanceId,
  driverKind: driver,
  continuationIdentity: {
    driverKind: driver,
    continuationKey: `codex:test:${instanceId}`,
  },
  displayName: String(instanceId),
  enabled: true,
  snapshot: {} as ProviderInstance["snapshot"],
  orchestrationAdapter,
  textGeneration: {} as ProviderInstance["textGeneration"],
});

const personalAdapter = makeAdapter(personalId);
const workAdapter = makeAdapter(workId);
const instances = [
  makeInstance(personalId, personalAdapter),
  makeInstance(workId, workAdapter),
] as const;
const instanceRegistryLayer = Layer.succeed(ProviderInstanceRegistry.ProviderInstanceRegistry, {
  getInstance: (instanceId) =>
    Effect.succeed(instances.find((instance) => instance.instanceId === instanceId)),
  listInstances: Effect.succeed(instances),
  listUnavailable: Effect.succeed([]),
  listEnvironments: Effect.succeed(new Map()),
  streamChanges: Stream.empty,
  subscribeChanges: Effect.never,
  rebuildInstanceWhen: () => Effect.succeed(false),
});
const TestLayer = ProviderAdapterRegistry.layerFromProviderInstanceRegistry.pipe(
  Layer.provide(instanceRegistryLayer),
);

it.effect("routes two configured instances of the same driver independently", () =>
  Effect.gen(function* () {
    const registry = yield* ProviderAdapterRegistry.ProviderAdapterRegistryV2;

    assert.strictEqual(yield* registry.get(personalId), personalAdapter);
    assert.strictEqual(yield* registry.get(workId), workAdapter);
    assert.deepEqual(yield* registry.list(), [personalId, workId]);
  }).pipe(Effect.provide(TestLayer)),
);

const lifecycleDriver = ProviderDriverKind.make("lifecycle-test");
const lifecycleInstanceId = ProviderInstanceId.make("lifecycle-test");
const lifecycleConfigMap: ProviderInstanceConfigMap = {
  [lifecycleInstanceId]: {
    driver: lifecycleDriver,
    config: {},
  },
};
const lifecycleAdapter = makeAdapter(lifecycleInstanceId);

const makeLifecycleDriver = (
  create: Effect.Effect<ProviderAdapterV2Shape, ProviderAdapterDriverCreateError, Scope.Scope>,
): ProviderAdapterDriver<Record<string, never>> => ({
  driverKind: lifecycleDriver,
  configSchema: Schema.Struct({}),
  defaultConfig: () => ({}),
  create: () => create,
});

const trackedCreate = <A, E, R>(
  releases: Ref.Ref<number>,
  effect: Effect.Effect<A, E, R>,
): Effect.Effect<A, E, R | Scope.Scope> =>
  Effect.gen(function* () {
    yield* Effect.addFinalizer(() => Ref.update(releases, (count) => count + 1));
    return yield* effect;
  });

it.effect("closes a partially-created adapter scope immediately on typed failure", () =>
  Effect.gen(function* () {
    const releases = yield* Ref.make(0);
    const createError = new ProviderAdapterDriverCreateError({
      driver: lifecycleDriver,
      instanceId: lifecycleInstanceId,
      detail: "expected test failure",
    });

    yield* Effect.scoped(
      Effect.gen(function* () {
        const exit = yield* ProviderAdapterRegistry.makeRegistryFromConfigMap({
          drivers: [makeLifecycleDriver(trackedCreate(releases, Effect.fail(createError)))],
          configMap: lifecycleConfigMap,
        }).pipe(Effect.exit);

        assert.isTrue(Exit.isFailure(exit));
        assert.strictEqual(yield* Ref.get(releases), 1);
      }),
    );

    assert.strictEqual(yield* Ref.get(releases), 1);
  }),
);

it.effect("closes a partially-created adapter scope immediately on defect", () =>
  Effect.gen(function* () {
    const releases = yield* Ref.make(0);

    yield* Effect.scoped(
      Effect.gen(function* () {
        const exit = yield* ProviderAdapterRegistry.makeRegistryFromConfigMap({
          drivers: [
            makeLifecycleDriver(trackedCreate(releases, Effect.die("expected test defect"))),
          ],
          configMap: lifecycleConfigMap,
        }).pipe(Effect.exit);

        assert.isTrue(Exit.hasDies(exit));
        assert.strictEqual(yield* Ref.get(releases), 1);
      }),
    );

    assert.strictEqual(yield* Ref.get(releases), 1);
  }),
);

it.effect("closes a partially-created adapter scope immediately on interruption", () =>
  Effect.gen(function* () {
    const releases = yield* Ref.make(0);
    const createStarted = yield* Deferred.make<void>();

    yield* Effect.scoped(
      Effect.gen(function* () {
        const fiber = yield* ProviderAdapterRegistry.makeRegistryFromConfigMap({
          drivers: [
            makeLifecycleDriver(
              trackedCreate(
                releases,
                Deferred.succeed(createStarted, undefined).pipe(Effect.andThen(Effect.never)),
              ),
            ),
          ],
          configMap: lifecycleConfigMap,
        }).pipe(Effect.forkChild({ startImmediately: true }));

        yield* Deferred.await(createStarted);
        yield* Fiber.interrupt(fiber);
        const exit = yield* Fiber.await(fiber);

        assert.isTrue(Exit.hasInterrupts(exit));
        assert.strictEqual(yield* Ref.get(releases), 1);
      }),
    );

    assert.strictEqual(yield* Ref.get(releases), 1);
  }),
);

it.effect("keeps a successfully-created adapter scope open until normal release", () =>
  Effect.gen(function* () {
    const releases = yield* Ref.make(0);

    yield* Effect.scoped(
      Effect.gen(function* () {
        const registry = yield* ProviderAdapterRegistry.makeRegistryFromConfigMap({
          drivers: [makeLifecycleDriver(trackedCreate(releases, Effect.succeed(lifecycleAdapter)))],
          configMap: lifecycleConfigMap,
        });

        assert.strictEqual(yield* registry.get(lifecycleInstanceId), lifecycleAdapter);
        assert.strictEqual(yield* Ref.get(releases), 0);
      }),
    );

    assert.strictEqual(yield* Ref.get(releases), 1);
  }),
);

it.effect(
  "blocks a new session while another instance changes their shared provider credentials",
  () =>
    Effect.gen(function* () {
      const unused = () => Effect.die("unused auth operation");
      const auth: ProviderAuthController = {
        credentialBinding: { owner: "provider", key: "shared-cli" },
        isChangingCredentials: Effect.succeed(false),
        start: unused,
        complete: unused,
        cancel: unused,
        logout: unused,
        subscribe: () => Stream.empty,
      };
      const related = [
        { ...instances[0], auth },
        { ...instances[1], auth: { ...auth, isChangingCredentials: Effect.succeed(true) } },
      ];
      const registry = yield* Effect.service(
        ProviderAdapterRegistry.ProviderAdapterRegistryV2,
      ).pipe(
        Effect.provide(
          ProviderAdapterRegistry.layerFromProviderInstanceRegistry.pipe(
            Layer.provide(
              Layer.mock(ProviderInstanceRegistry.ProviderInstanceRegistry)({
                getInstance: (id) =>
                  Effect.succeed(related.find((instance) => instance.instanceId === id)),
                listInstances: Effect.succeed(related),
              }),
            ),
          ),
        ),
      );
      const adapter = yield* registry.get(personalId);
      const error = yield* adapter
        .openSession({
          threadId: ThreadId.make("new-thread"),
          providerSessionId: ProviderSessionId.make("new-session"),
          modelSelection: { instanceId: personalId, model: "test-model" },
          runtimePolicy: {
            runtimeMode: "full-access",
            interactionMode: "default",
            cwd: "/workspace",
          },
        })
        .pipe(Effect.flip);
      assert.instanceOf(error, ProviderAdapterOpenSessionError);
      assert.instanceOf(error.cause, ProviderSetupError);
    }),
);

it.effect("interrupts admitted session startup when a shared peer signs out", () =>
  Effect.gen(function* () {
    const entered = yield* Deferred.make<void>();
    const stopped = yield* Deferred.make<void>();
    const binding = { owner: "provider" as const, key: "shared-cli" };
    const auth = yield* ProviderAuthFlow.make({
      instanceId: personalId,
      credentialBinding: binding,
      methods: Effect.succeed([]),
      authenticate: () => Effect.void,
      logout: Effect.void,
    });
    const peerAuth = yield* ProviderAuthFlow.make({
      instanceId: workId,
      credentialBinding: binding,
      methods: Effect.succeed([]),
      authenticate: () => Effect.void,
      logout: Effect.void,
    });
    const adapter: ProviderAdapterV2Shape = {
      ...workAdapter,
      openSession: () =>
        Effect.gen(function* () {
          yield* Deferred.succeed(entered, undefined);
          return yield* Effect.never;
        }).pipe(Effect.ensuring(Deferred.succeed(stopped, undefined))),
    };
    const related = [
      { ...instances[0], auth },
      { ...instances[1], auth: peerAuth, orchestrationAdapter: adapter },
    ];
    const registry = yield* Effect.service(ProviderAdapterRegistry.ProviderAdapterRegistryV2).pipe(
      Effect.provide(
        ProviderAdapterRegistry.layerFromProviderInstanceRegistry.pipe(
          Layer.provide(
            Layer.mock(ProviderInstanceRegistry.ProviderInstanceRegistry)({
              getInstance: (id) =>
                Effect.succeed(related.find((instance) => instance.instanceId === id)),
              listInstances: Effect.succeed(related),
            }),
          ),
        ),
      ),
    );
    const guarded = yield* registry.get(workId);
    const startup = yield* guarded
      .openSession({
        threadId: ThreadId.make("shared-startup"),
        providerSessionId: ProviderSessionId.make("shared-session"),
        modelSelection: { instanceId: workId, model: "test-model" },
        runtimePolicy: {
          runtimeMode: "full-access",
          interactionMode: "default",
          cwd: "/workspace",
        },
      })
      .pipe(Effect.forkChild);
    yield* Deferred.await(entered);
    yield* auth.logout(Effect.void);
    yield* Deferred.await(stopped);
    assert.isTrue(Exit.isFailure(yield* Fiber.await(startup)));
  }).pipe(Effect.provide(NodeServices.layer)),
);

it.effect("opens v2 sessions with resolved secrets and rebuilds them when a secret rotates", () =>
  Effect.gen(function* () {
    const secretInstanceId = ProviderInstanceId.make("codex_secret");
    const apiKeyReference = "op://Vault/Codex/api-key";
    const apiKey = yield* Ref.make("sk-first");
    const sessionEnvironments = yield* Ref.make<ReadonlyArray<NodeJS.ProcessEnv>>([]);
    const resolver: ProviderSecretResolver.ProviderSecretResolverShape = {
      resolve: (environment) =>
        Effect.gen(function* () {
          const variables = [];
          const unresolved = [];
          for (const { name, value, sensitive } of environment ?? []) {
            if (typeof value !== "string" && value.reference === apiKeyReference) {
              variables.push({ name, value: yield* Ref.get(apiKey), sensitive: true });
            } else if (typeof value !== "string") {
              unresolved.push(name);
            } else {
              variables.push({ name, value, sensitive });
            }
          }
          return { variables, unresolved };
        }),
      prime: () => Effect.void,
      invalidate: Effect.void,
      listOnePasswordAccounts: Effect.succeed([]),
    };
    const secretDriver: ProviderDriver<Record<string, never>> = {
      driverKind: driver,
      metadata: { displayName: "Codex" },
      configSchema: Schema.Struct({}),
      defaultConfig: () => ({}),
      create: (input) =>
        Effect.gen(function* () {
          const environment = mergeProviderInstanceEnvironment(
            input.environment,
            yield* HostProcessEnvironment,
          );
          return {
            ...makeInstance(input.instanceId, {
              ...makeAdapter(input.instanceId),
              openSession: () =>
                Ref.update(sessionEnvironments, (previous) => [...previous, environment]).pipe(
                  Effect.andThen(Effect.die("session environment recorded")),
                ),
            }),
            snapshot: {
              getSnapshot: Effect.succeed({}),
            } as unknown as ProviderInstance["snapshot"],
          };
        }),
    };
    const openSession = (registry: ProviderAdapterRegistry.ProviderAdapterRegistryV2["Service"]) =>
      registry.get(secretInstanceId).pipe(
        Effect.flatMap((adapter) =>
          adapter.openSession({
            threadId: ThreadId.make("secret-thread"),
            providerSessionId: ProviderSessionId.make("secret-session"),
            modelSelection: { instanceId: secretInstanceId, model: "test-model" },
            runtimePolicy: {
              runtimeMode: "full-access",
              interactionMode: "default",
              cwd: "/workspace",
            },
          }),
        ),
        Effect.exit,
      );

    const { registry: instanceRegistry } = yield* makeProviderInstanceRegistry({
      drivers: [secretDriver],
      configMap: {
        [secretInstanceId]: {
          driver,
          environment: decodeEnvironment([
            onePasswordVariable("OPENAI_API_KEY", apiKeyReference),
            onePasswordVariable("ANTHROPIC_API_KEY", "op://Vault/Locked/api-key"),
            { name: "CODEX_PROFILE", value: "work", sensitive: false },
          ]),
          config: {},
        },
      },
    }).pipe(
      Effect.provideService(ProviderSecretResolver.ProviderSecretResolver, resolver),
      Effect.provideService(HostProcessEnvironment, {
        PATH: "/bin",
        ANTHROPIC_API_KEY: "inherited-key",
      }),
    );
    const registry = yield* Effect.service(ProviderAdapterRegistry.ProviderAdapterRegistryV2).pipe(
      Effect.provide(
        ProviderAdapterRegistry.layerFromProviderInstanceRegistry.pipe(
          Layer.provide(
            Layer.succeed(ProviderInstanceRegistry.ProviderInstanceRegistry, instanceRegistry),
          ),
        ),
      ),
    );

    yield* openSession(registry);
    yield* Ref.set(apiKey, "sk-rotated");
    assert.isTrue(
      yield* instanceRegistry.rebuildInstanceWhen(secretInstanceId, (entry) =>
        hasProviderSecretReference(entry.environment),
      ),
    );
    yield* openSession(registry);

    const [first, rotated] = yield* Ref.get(sessionEnvironments);
    assert.deepEqual(first, {
      PATH: "/bin",
      OPENAI_API_KEY: "sk-first",
      CODEX_PROFILE: "work",
    });
    assert.strictEqual(rotated?.OPENAI_API_KEY, "sk-rotated");
  }).pipe(Effect.scoped),
);
