/**
 * ProviderSecretResolver: turns `op://` environment values into the secrets
 * they name, once, and holds them in memory.
 *
 * Every provider instance resolves its environment when the driver builds it,
 * and a single instance can rebuild several times per session. Shelling out
 * to `op` on each of those is slow (seconds) and, worse, can put a biometric
 * prompt in front of a user who only started a thread. The resolver therefore
 * caches by reference for the lifetime of the process; `invalidate` is wired
 * to the Settings refresh button, which is the user's way of saying "go read
 * it again" after rotating a credential.
 *
 * @module provider/Services/ProviderSecretResolver
 */
import type { ProviderInstanceEnvironment } from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import type { ResolvedProviderInstanceEnvironment } from "../ProviderInstanceEnvironment.ts";

export interface ProviderSecretResolverShape {
  /**
   * Replace every secret reference in the environment with its value.
   * Literal values pass through untouched, and an environment with no
   * references is returned as-is.
   *
   * Never fails. A reference that cannot be read (1Password locked, `op` not
   * installed, item deleted) is reported as unresolved rather than
   * substituted with an empty string, so the provider reports the honest
   * "unauthenticated" instead of failing later with a credential that looks
   * present and is not. `mergeProviderInstanceEnvironment` unsets those names,
   * which is what keeps the provider off a same-named credential the server
   * happens to have inherited.
   */
  readonly resolve: (
    environment: ProviderInstanceEnvironment | undefined,
  ) => Effect.Effect<ResolvedProviderInstanceEnvironment>;
  /**
   * Drop every cached secret. The next `resolve` re-reads from the store.
   * Callers that need the new value to reach a running provider must also
   * rebuild the instance: a provider process keeps the environment it was
   * spawned with.
   */
  readonly invalidate: Effect.Effect<void>;
}

export class ProviderSecretResolver extends Context.Service<
  ProviderSecretResolver,
  ProviderSecretResolverShape
>()("t3/provider/Services/ProviderSecretResolver") {}

/**
 * Resolver that hands every environment back untouched. This is what a build
 * without secret-store integration behaves like, and what tests want unless
 * they are testing resolution itself: an `op://` value stays an `op://`
 * value, and the provider reports whatever the CLI makes of it.
 */
export const passthroughProviderSecretResolver: ProviderSecretResolverShape = {
  resolve: (environment) => Effect.succeed({ variables: environment, unresolved: [] }),
  invalidate: Effect.void,
};

export const ProviderSecretResolverPassthroughLayer: Layer.Layer<ProviderSecretResolver> =
  Layer.succeed(ProviderSecretResolver, passthroughProviderSecretResolver);
