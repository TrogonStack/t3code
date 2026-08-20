/**
 * ProviderInstanceRegistry — the single Effect service in the new model.
 *
 * Owns a `Map<ProviderInstanceId, ProviderInstance>` produced by running
 * registered driver factories against `ServerSettings.providerInstances`.
 * The registry watches settings; when an instance's config changes (or
 * the entry disappears), the registry tears down the affected instance's
 * scope and rebuilds — that's the entire hot-reload story.
 *
 * What rest-of-server reads from here:
 *   - `getInstance(instanceId)` — for routing turn/session calls.
 *   - `listInstances` — for snapshot aggregation in `ProviderRegistry`.
 *   - `listUnavailable` — `ServerProvider` shadows for instances whose
 *     driver is not registered in this build (rollback / fork tolerance).
 *   - `streamChanges` — coalesced "registry mutated" pings so consumers
 *     can re-pull lists or re-broadcast.
 *
 * @module provider/Services/ProviderInstanceRegistry
 */
import type {
  ProviderInstanceConfig,
  ProviderInstanceEnvironment,
  ProviderInstanceId,
  ServerProvider,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import type * as Effect from "effect/Effect";
import type * as PubSub from "effect/PubSub";
import type * as Scope from "effect/Scope";
import type * as Stream from "effect/Stream";

import type { ProviderInstance } from "../ProviderDriver.ts";

export interface ProviderInstanceRegistryShape {
  /**
   * Look up one instance by id. Returns `undefined` (not Option) when the
   * id is unknown — callers branch on falsy and emit
   * `ProviderInstanceNotFoundError`.
   */
  readonly getInstance: (
    instanceId: ProviderInstanceId,
  ) => Effect.Effect<ProviderInstance | undefined>;
  /**
   * Every available (driver-registered, successfully created) instance,
   * in stable settings-author order.
   */
  readonly listInstances: Effect.Effect<ReadonlyArray<ProviderInstance>>;
  /**
   * Wire-shape shadow snapshots for instances whose driver is unknown to
   * this build (or whose config failed to decode). Suitable for merging
   * directly into `ProviderRegistry` output.
   */
  readonly listUnavailable: Effect.Effect<ReadonlyArray<ServerProvider>>;
  /**
   * The configured environment of every instance the registry could rebuild,
   * keyed by id. Covers the live instances and the ones whose last build
   * failed, which is exactly the set `rebuildInstanceWhen` can act on.
   *
   * Exists so a caller that is about to rebuild several instances can inspect
   * what they are configured with first. `ProviderRegistry` uses it to resolve
   * every external secret in one go instead of once per rebuild; the registry
   * itself stays ignorant of what any particular value means.
   */
  readonly listEnvironments: Effect.Effect<
    ReadonlyMap<ProviderInstanceId, ProviderInstanceEnvironment | undefined>
  >;
  /**
   * Tear one instance down and build it again from the configuration it
   * already has, but only when `shouldRebuild` accepts that configuration.
   * Resolves to whether a rebuild happened.
   *
   * This exists for inputs a driver reads at create time that can change
   * without the settings file changing - a credential held in an external
   * secret store, say. `reconcile` cannot see those, because the config
   * envelope it diffs is identical before and after. The predicate keeps the
   * decision with the caller that understands the input, and keeps every
   * other instance out of the blast radius.
   *
   * The rebuilt instance keeps its position in settings-author order, and an
   * unknown id is a no-op rather than an error. Sessions already talking to
   * the old instance keep the process they were given; only work started
   * after the rebuild sees the new one.
   */
  readonly rebuildInstanceWhen: (
    instanceId: ProviderInstanceId,
    shouldRebuild: (entry: ProviderInstanceConfig) => boolean,
  ) => Effect.Effect<boolean>;
  /**
   * Push notification stream emitted whenever the registry's contents
   * change — instance added, removed, or rebuilt. The payload is `void`
   * because consumers always want to re-pull `listInstances` /
   * `listUnavailable` together.
   *
   * NOTE: because `Stream.fromPubSub` defers `PubSub.subscribe` until the
   * stream starts running, forking a consumer via
   * `Stream.runForEach(...).pipe(Effect.forkScoped)` races the next
   * publish — the forked fiber may not have subscribed yet when the
   * publish lands. Hot-reload consumers that must not miss a publish
   * should use `subscribeChanges` below instead, which acquires the
   * subscription synchronously in the caller's fiber before the consumer
   * loop is forked.
   */
  readonly streamChanges: Stream.Stream<void>;
  /**
   * Acquire a subscription to the registry's change channel synchronously
   * in the caller's fiber. Returns a `PubSub.Subscription<void>` whose
   * lifetime is scoped to the provided `Scope` (the subscription is
   * released when the scope closes). Consumers typically `yield*` this
   * in the same fiber that forks their consumer loop, then drain with
   * `PubSub.take(subscription)` inside `Effect.forever`. Because the
   * subscription is registered with the PubSub before this `yield*`
   * returns, no subsequent publish can land in a gap.
   *
   * This exists because the `ProviderInstanceRegistry` publishes on a
   * PubSub and `Stream.fromPubSub` defers subscription until the stream
   * starts executing — a consumer that `forkScoped`s the stream
   * consumption can miss a publish that lands in the narrow window
   * between "fiber scheduled" and "fiber starts running".
   */
  readonly subscribeChanges: Effect.Effect<PubSub.Subscription<void>, never, Scope.Scope>;
}

export class ProviderInstanceRegistry extends Context.Service<
  ProviderInstanceRegistry,
  ProviderInstanceRegistryShape
>()("t3/provider/Services/ProviderInstanceRegistry") {}
