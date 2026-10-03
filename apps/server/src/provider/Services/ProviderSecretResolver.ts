/**
 * ProviderSecretResolver: turns environment values that name a secret (such as
 * a 1Password secret source) into the secrets they name, once, and holds them
 * in memory.
 *
 * Every provider instance resolves its environment when the driver builds it,
 * and a single instance can rebuild several times per session. Shelling out
 * to `op` on each of those is slow (seconds) and, worse, can put a biometric
 * prompt in front of a user who only started a thread. The resolver therefore
 * caches by secret (reference plus account) for the lifetime of the process; `invalidate` is wired
 * to the Settings refresh button, which is the user's way of saying "go read
 * it again" after rotating a credential.
 *
 * @module provider/Services/ProviderSecretResolver
 */
import type { ProviderInstanceEnvironment } from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";

import type { ResolvedProviderEnvironment } from "../ProviderInstanceEnvironment.ts";
import type { ProviderSecretReference } from "../ProviderSecretReference.ts";

/**
 * An instance environment after its secret references have been read.
 *
 * `unresolved` names the variables the instance configures but whose value
 * could not be read. They are tracked separately because being absent from
 * `variables` is not enough: the child environment starts from the server's
 * own, so a name left alone keeps whatever the server inherited under it.
 * That is the wrong credential precisely when the user asked for a specific
 * one, and it hides as "authenticated".
 */
export interface ResolvedProviderInstanceEnvironment {
  readonly variables: ResolvedProviderEnvironment;
  readonly unresolved: ReadonlyArray<string>;
}

export interface ProviderSecretResolverShape {
  /**
   * Replace every secret the environment names with its value. Literal values
   * pass through untouched.
   *
   * Never fails. A reference that cannot be read (1Password locked, `op` not
   * installed, item deleted) is reported as unresolved rather than
   * substituted with an empty string, so the provider reports the honest
   * "unauthenticated" instead of failing later with a credential that looks
   * present and is not. The instance registry unsets those names,
   * which is what keeps the provider off a same-named credential the server
   * happens to have inherited.
   */
  readonly resolve: (
    environment: ProviderInstanceEnvironment | undefined,
  ) => Effect.Effect<ResolvedProviderInstanceEnvironment>;
  /**
   * Resolve `references` ahead of the callers that will ask for them, in as
   * few reads as the secret store allows.
   *
   * Unlocking is charged per invocation of the store's CLI, not per secret, so
   * a caller about to build ten instances that each resolve their own
   * environment would otherwise cost ten authorizations. Priming turns that
   * into one.
   *
   * Purely an optimization, and deliberately best effort: it never fails, and
   * a batch that does not come back leaves the cache exactly as cold as it
   * found it, so `resolve` falls back to reading one reference at a time with
   * the same per-variable failure isolation it has always had.
   */
  readonly prime: (references: ReadonlyArray<ProviderSecretReference>) => Effect.Effect<void>;
  /**
   * Drop every cached secret. The next `resolve` re-reads from the store.
   * Callers that need the new value to reach a running provider must also
   * rebuild the instance: a provider process keeps the environment it was
   * spawned with.
   */
  readonly invalidate: Effect.Effect<void>;
}

/**
 * Defaults to handing every literal back untouched, which is what a build
 * without secret-store integration behaves like, and what tests want unless
 * they are testing resolution itself. A secret source has no literal form, so
 * it is reported unresolved.
 */
const resolveWithoutSecretStore = (
  environment: ProviderInstanceEnvironment | undefined,
): ResolvedProviderInstanceEnvironment => {
  const variables: Array<ResolvedProviderEnvironment[number]> = [];
  const unresolved: Array<string> = [];
  for (const { name, value, sensitive } of environment ?? []) {
    if (typeof value === "string") {
      variables.push({ name, value, sensitive });
    } else {
      unresolved.push(name);
    }
  }
  return { variables, unresolved };
};

export class ProviderSecretResolver extends Context.Reference<ProviderSecretResolverShape>(
  "t3/provider/Services/ProviderSecretResolver",
  {
    defaultValue: () => ({
      resolve: (environment) => Effect.sync(() => resolveWithoutSecretStore(environment)),
      prime: () => Effect.void,
      invalidate: Effect.void,
    }),
  },
) {}
