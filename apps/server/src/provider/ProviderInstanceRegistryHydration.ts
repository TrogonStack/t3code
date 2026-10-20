import {
  defaultInstanceIdForDriver,
  providerInstanceConfigEnabledFlag,
  resolveProviderInstanceEnabled,
  type ProviderInstanceConfig,
  type ProviderInstanceConfigMap,
  type ProviderInstanceId,
  type SecretProviderId,
  ServerSettings,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Ref from "effect/Ref";
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
import type * as ServerConfig from "../config.ts";

type ProviderInstanceRegistryHydrationEnv =
  | Exclude<
      BuiltInDriversEnv,
      | ProviderOrchestrationAdapterInfrastructure.ProviderOrchestrationAdapterInfrastructure
      | AcpRegistrySupport.AcpRegistryCatalog
      | ProviderHost.ProviderHost
    >
  | Settings.ServerSettingsService
  | BackgroundPolicy.BackgroundPolicy
  | ServerConfig.ServerConfig;

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

  // A project that turns an instance on needs it alive and probed even when the machine itself
  // has it off, or a thread in that project would route to an instance nobody ever checked in
  // on.
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
