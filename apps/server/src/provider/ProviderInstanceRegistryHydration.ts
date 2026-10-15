/**
 * ProviderInstanceRegistryHydration — derive a `ProviderInstanceConfigMap`
 * from `ServerSettings` and keep `ProviderInstanceRegistry` in sync with it.
 *
 * `settings.providerInstances` is the source of truth. Every built-in driver
 * with a default instance also runs at `defaultInstanceIdForDriver(kind)`
 * when that slot has no entry, using the driver's default config, so a fresh
 * install shows its built-in providers without writing settings first.
 *
 * Hot-reload
 * ----------
 * On layer build we:
 *   1. Read the current `ServerSettings` once and use it to seed the
 *      registry's initial state via `ProviderInstanceRegistry.layer`,
 *      priming that snapshot's secret references first so the whole boot
 *      fleet costs one unlock rather than one per instance.
 *   2. Fork a daemon fiber (lifetime tied to the layer's scope) that
 *      acquires `ServerSettingsService.subscribeChanges` and calls
 *      `ProviderInstanceRegistryMutator.reconcile` on every emission.
 *      When a secret provider was added, edited, or removed, the cached
 *      secrets are dropped first and every instance that reads through one
 *      of those providers is rebuilt afterwards, since its own config, and
 *      so `reconcile`'s diff, did not change.
 *
 * Failures inside the watcher are logged and swallowed so a single bad
 * settings emission cannot kill the registry. Unknown drivers and invalid
 * configs already round-trip through the registry's own "unavailable"
 * shadow bucket.
 *
 * @module provider/ProviderInstanceRegistryHydration
 */
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
  // Requirements of the `ProviderHost.ProviderHost` the drivers receive.
  | BackgroundPolicy.BackgroundPolicy
  | ServerConfig.ServerConfig;

/**
 * Explicit `providerInstances` entries plus an implicit default instance for
 * each built-in driver whose default slot is empty. Pure so the hydration
 * rule can be tested without layers.
 */
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

  // A project that turns an instance on needs it alive and probed even when
  // the machine itself has it off, or a thread in that project would route
  // to an instance nobody ever checked in on. Fold every project's
  // `providerInstances[id].enabled` overrides in: an instance already enabled
  // is untouched, and one absent from settings entirely (deleted) stays out.
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
 * Read every secret the config map's instances are about to need, before any
 * of them is built. Each instance resolves its own environment, and the secret
 * store charges an unlock per read rather than per secret, so without this a
 * fleet of five reference-backed providers is five authorizations.
 */
const primeConfigMapSecrets = (
  secretResolver: ProviderSecretResolver.ProviderSecretResolverShape,
  configMap: ProviderInstanceConfigMap,
) =>
  secretResolver.prime(
    collectProviderSecretReferences(Object.values(configMap).map((entry) => entry.environment)),
  );

/**
 * Ids of the secret providers that were added, removed, or edited between two
 * settings snapshots. Pure so the rule can be tested without layers.
 */
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

/**
 * Layer that consumes `ProviderInstanceRegistryMutator` and forks a
 * settings-watcher fiber. The fiber's lifetime is tied to the enclosing
 * layer scope (process lifetime in production), so it is interrupted on
 * shutdown without leaking.
 *
 * Errors inside the watcher are logged and swallowed — the registry's own
 * "unavailable" bucket already absorbs unknown drivers and invalid
 * configs, so the only way the watcher could fail is a settings stream
 * tear-down, which logs and exits cleanly.
 */
const layerSettingsWatcher = (initialSecretProviders: ServerSettings["secretProviders"]) =>
  Layer.effectDiscard(
    Effect.gen(function* () {
      const mutator = yield* ProviderInstanceRegistryMutator.ProviderInstanceRegistryMutator;
      const registry = yield* ProviderInstanceRegistry.ProviderInstanceRegistry;
      const serverSettings = yield* Settings.ServerSettingsService;
      const secretResolver = yield* ProviderSecretResolver.ProviderSecretResolver;
      const secretProvidersRef = yield* Ref.make(initialSecretProviders);
      const settingsChanges = yield* serverSettings.subscribeChanges;
      yield* settingsChanges.pipe(
        Stream.runForEach((next) =>
          Effect.gen(function* () {
            const configMap = deriveProviderInstanceConfigMap(next);
            const changedProviders = changedSecretProviderIds(
              yield* Ref.get(secretProvidersRef),
              next.secretProviders,
            );
            if (changedProviders.size > 0) {
              yield* secretResolver.invalidate;
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
          }).pipe(
            Effect.catchCause((cause) =>
              Effect.logError("ProviderInstanceRegistry reconcile failed", cause),
            ),
          ),
        ),
        Effect.forkScoped,
      );
    }),
  );

/**
 * Hydrate `ProviderInstanceRegistry` from `ServerSettings` and keep it in
 * sync with subsequent `subscribeChanges` emissions.
 *
 * The Layer's two halves:
 *   - `ProviderInstanceRegistry.layer` produces the registry +
 *     mutator from the initial config map. Its scope owns every
 *     per-instance child scope created during reconcile.
 *   - `SettingsWatcherLive` consumes the mutator, acquires its settings
 *     subscription before forking, and runs a daemon fiber in the same scope.
 *
 * Composing via `Layer.provideMerge` makes the watcher's deps available
 * from the mutable layer while still surfacing the registry as an output.
 * The mutator tag is technically also exposed; only this module imports
 * it, so the visibility leak is harmless in practice.
 */
export const layer: Layer.Layer<
  ProviderInstanceRegistry.ProviderInstanceRegistry,
  never,
  ProviderInstanceRegistryHydrationEnv
> = Layer.unwrap(
  Effect.gen(function* () {
    const serverSettings = yield* Settings.ServerSettingsService;
    const secretResolver = yield* ProviderSecretResolver.ProviderSecretResolver;
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

    return layerSettingsWatcher(initialSettings?.secretProviders ?? {}).pipe(
      Layer.provideMerge(layerMutable),
    );
  }),
) as Layer.Layer<
  ProviderInstanceRegistry.ProviderInstanceRegistry,
  never,
  ProviderInstanceRegistryHydrationEnv
>;
