import {
  defaultInstanceIdForDriver,
  ProviderDriverKind,
  ProviderInstanceId,
  providerInstanceConfigEnabledFlag,
  resolveProviderInstanceEnabled,
  type ProviderInstanceConfig,
  type ProviderInstanceConfigMap,
  type SecretProviderId,
  ServerSettings,
} from "@t3tools/contracts";
import * as HostProcess from "@t3tools/shared/HostProcess";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Ref from "effect/Ref";
import * as Path from "effect/Path";
import * as Stream from "effect/Stream";

import * as Settings from "../serverSettings.ts";
import { BUILT_IN_DRIVERS, type BuiltInDriversEnv } from "./builtInDrivers.ts";
import {
  collectProviderSecretReferences,
  providerSecretReference,
} from "./ProviderSecretReference.ts";
import * as ProviderInstanceRegistry from "./ProviderInstanceRegistry.ts";
import * as ProviderInstanceRegistryMutator from "./ProviderInstanceRegistryMutator.ts";
import * as ProviderSecretResolver from "./ProviderSecretResolver.ts";
import * as ProviderOrchestrationAdapterInfrastructure from "./ProviderOrchestrationAdapterInfrastructure.ts";
import * as AcpRegistrySupport from "@t3tools/provider-acp-registry/server/AcpRegistrySupport";
import * as ProviderHostLive from "./ProviderHostLive.ts";
import type * as ProviderHost from "@t3tools/provider-core/server/ProviderHost";
import type * as BackgroundPolicy from "../background/BackgroundPolicy.ts";
import * as ServerConfig from "../config.ts";
import type * as PortScanner from "../preview/PortScanner.ts";

type ProviderInstanceRegistryHydrationEnv =
  | Exclude<
      BuiltInDriversEnv,
      | ProviderOrchestrationAdapterInfrastructure.ProviderOrchestrationAdapterInfrastructure
      | AcpRegistrySupport.AcpRegistryCatalog
      | ProviderHost.ProviderHost
    >
  | Settings.ServerSettingsService
  | BackgroundPolicy.BackgroundPolicy
  | ServerConfig.ServerConfig
  | Path.Path
  | FileSystem.FileSystem
  // The Claude runner counts its CLIs as running agents for port discovery.
  | PortScanner.PortDiscovery;

export const deriveProviderInstanceConfigMap = (
  settings: ServerSettings,
): ProviderInstanceConfigMap => {
  const merged: Record<string, ProviderInstanceConfig> = { ...settings.providerInstances };

  for (const driver of BUILT_IN_DRIVERS) {
    if (driver.metadata.hasDefaultInstance === false) continue;
    const instanceId = defaultInstanceIdForDriver(driver.driverKind);
    if (instanceId in merged) continue;
    merged[instanceId] = { driver: driver.driverKind };
  }

  // Probe project-enabled instances even if disabled machine-wide; project threads can still route to them.
  for (const overrides of Object.values(settings.projectSettingsOverrides)) {
    for (const [rawInstanceId, projectOverride] of Object.entries(
      overrides?.providerInstances ?? {},
    )) {
      if (projectOverride.enabled !== true) continue;
      const instanceId = rawInstanceId as ProviderInstanceId;
      const existing = merged[instanceId];
      if (existing === undefined || resolveProviderInstanceEnabled(existing)) continue;
      merged[instanceId] = {
        ...existing,
        enabled: true,
        ...(providerInstanceConfigEnabledFlag(existing.config) === false
          ? { config: { ...(existing.config as Record<string, unknown>), enabled: true } }
          : {}),
      };
    }
  }

  return merged as ProviderInstanceConfigMap;
};

export const DEV_REPLAY_INSTANCE_ID = ProviderInstanceId.make("devReplay");

/**
 * Dev servers run from source keep a free ACP agent that replays recorded turns, so UI checks
 * need no paid model. Bundled servers, such as desktop dev, have no agent script and skip it.
 */
export const seedDevReplayInstance = Effect.fn(
  "ProviderInstanceRegistryHydration.seedDevReplayInstance",
)(function* () {
  const config = yield* ServerConfig.ServerConfig;
  if (config.devUrl === undefined) return;
  const path = yield* Path.Path;
  const agentPath = yield* path.fromFileUrl(
    new URL("../../scripts/dev-replay-agent.ts", import.meta.url),
  );
  if (!(yield* (yield* FileSystem.FileSystem).exists(agentPath))) return;
  const commandPath = yield* HostProcess.ExecutablePath;
  const serverSettings = yield* Settings.ServerSettingsService;
  const existing = (yield* serverSettings.getSettings).providerInstances[DEV_REPLAY_INSTANCE_ID];
  const existingConfig = existing?.config as Record<string, unknown> | undefined;
  // Rewritten only when stale: the node and checkout paths can move.
  if (
    existingConfig?.commandPath === commandPath &&
    (existingConfig.commandArgs as ReadonlyArray<unknown> | undefined)?.[0] === agentPath
  ) {
    return;
  }
  // Keeps user edits, such as turning the provider off.
  const instance: ProviderInstanceConfig = {
    displayName: "Replay (dev)",
    ...existing,
    driver: ProviderDriverKind.make("acpRegistry"),
    config: { ...existingConfig, source: "local", commandPath, commandArgs: [agentPath] },
  };
  yield* serverSettings.updateProviderInstance({
    operation: "upsert",
    instanceId: DEV_REPLAY_INSTANCE_ID,
    instance,
  });
});

/**
 * Reads every secret the config map's instances need before any is built, since the secret store
 * charges an unlock per read rather than per secret.
 */
const primeConfigMapSecrets = (
  secretResolver: ProviderSecretResolver.ProviderSecretResolverShape,
  configMap: ProviderInstanceConfigMap,
) =>
  secretResolver.prime(
    collectProviderSecretReferences(Object.values(configMap).map((entry) => entry.environment)),
  );

/** Ids of the secret providers added, removed, or edited between two settings snapshots. */
export const changedSecretProviderIds = (
  previous: ServerSettings["secretProviders"],
  next: ServerSettings["secretProviders"],
): ReadonlySet<SecretProviderId> => {
  const changed = new Set<SecretProviderId>();
  for (const id of new Set([...Object.keys(previous), ...Object.keys(next)])) {
    const providerId = id as SecretProviderId;
    if (JSON.stringify(previous[providerId]) !== JSON.stringify(next[providerId])) {
      changed.add(providerId);
    }
  }
  return changed;
};

const readsThroughSecretProvider = (
  entry: ProviderInstanceConfig,
  providers: ReadonlySet<SecretProviderId>,
) =>
  entry.environment?.some((variable) => {
    const reference = providerSecretReference(variable.value);
    return reference?._tag === "secret-provider" && providers.has(reference.provider);
  }) ?? false;

const layerSettingsWatcher = (
  settingsChanges: Stream.Stream<ServerSettings>,
  initialSecretProviders: ServerSettings["secretProviders"],
) =>
  Layer.effectDiscard(
    Effect.gen(function* () {
      const mutator = yield* ProviderInstanceRegistryMutator.ProviderInstanceRegistryMutator;
      const registry = yield* ProviderInstanceRegistry.ProviderInstanceRegistry;
      const serverSettings = yield* Settings.ServerSettingsService;
      const secretResolver = yield* ProviderSecretResolver.ProviderSecretResolver;
      const secretProvidersRef = yield* Ref.make(initialSecretProviders);
      yield* settingsChanges.pipe(
        Stream.runForEach(() =>
          // The subscription predates the seed read, so a queued change can be
          // older than the seed. A failed current read must leave the registry intact.
          serverSettings.getSettings.pipe(
            Effect.flatMap((next) =>
              Effect.gen(function* () {
                const configMap = deriveProviderInstanceConfigMap(next);
                const changedProviders = changedSecretProviderIds(
                  yield* Ref.get(secretProvidersRef),
                  next.secretProviders,
                );
                if (changedProviders.size > 0) {
                  yield* secretResolver.invalidate(changedProviders);
                }
                yield* primeConfigMapSecrets(secretResolver, configMap);
                yield* mutator.reconcile(configMap);
                if (changedProviders.size > 0) {
                  const environments = yield* registry.listEnvironments;
                  yield* Effect.forEach(
                    environments.keys(),
                    (instanceId) =>
                      registry.rebuildInstanceWhen(instanceId, (entry) =>
                        readsThroughSecretProvider(entry, changedProviders),
                      ),
                    { discard: true },
                  );
                }
                // Committed last so a failed reconcile is retried on the next emission
                // instead of being treated as already applied.
                yield* Ref.set(secretProvidersRef, next.secretProviders);
              }),
            ),
            Effect.catchCause((cause) =>
              Effect.logError("ProviderInstanceRegistry reconcile failed", cause),
            ),
          ),
        ),
        Effect.forkScoped,
      );
    }),
  );

export const layer: Layer.Layer<
  ProviderInstanceRegistry.ProviderInstanceRegistry,
  never,
  ProviderInstanceRegistryHydrationEnv
> = Layer.unwrap(
  Effect.gen(function* () {
    const serverSettings = yield* Settings.ServerSettingsService;
    const secretResolver = yield* ProviderSecretResolver.ProviderSecretResolver;
    yield* seedDevReplayInstance().pipe(
      Effect.catchCause((cause) => Effect.logWarning("Dev replay agent unavailable", cause)),
    );
    // Subscribe before the seed read so saves during registry construction are queued.
    const settingsChanges = yield* serverSettings.subscribeChanges;
    const initialSettings: ServerSettings | undefined = yield* serverSettings.getSettings.pipe(
      Effect.orElseSucceed(() => undefined),
    );
    const initialConfigMap =
      initialSettings === undefined
        ? ({} as ProviderInstanceConfigMap)
        : deriveProviderInstanceConfigMap(initialSettings);

    // The watcher only sees later writes, so this snapshot is the whole boot
    // fleet and the one place where nothing is cached yet.
    yield* primeConfigMapSecrets(secretResolver, initialConfigMap);

    const layerMutable = ProviderInstanceRegistry.layer({
      drivers: BUILT_IN_DRIVERS,
      configMap: initialConfigMap,
    }).pipe(
      Layer.provide(ProviderOrchestrationAdapterInfrastructure.layer),
      Layer.provide(AcpRegistrySupport.layerFromHost),
      Layer.provide(ProviderHostLive.layer),
    );

    return layerSettingsWatcher(settingsChanges, initialSettings?.secretProviders ?? {}).pipe(
      Layer.provideMerge(layerMutable),
    );
  }),
) as Layer.Layer<
  ProviderInstanceRegistry.ProviderInstanceRegistry,
  never,
  ProviderInstanceRegistryHydrationEnv
>;
