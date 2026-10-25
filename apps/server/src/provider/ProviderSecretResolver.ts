/**
 * ProviderSecretResolver: resolves secret-naming environment values once and caches them per
 * secret (reference plus account) for the process lifetime, so rebuilds do not re-run `op` or
 * re-prompt. `invalidate` is wired to the Settings refresh button.
 * @module provider/ProviderSecretResolver
 */
import {
  type OnePasswordAccountSummary,
  type ProviderInstanceEnvironment,
  type ProviderSecretSource,
  SecretProviderId,
  type SecretSourceCheckResult,
  type SettingsSecretValue,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

import type { ResolvedProviderEnvironment } from "@t3tools/provider-core/server/instanceEnvironment";
import type { ProviderSecretReference } from "./ProviderSecretReference.ts";

/**
 * An instance environment after its secret references were read. `unresolved` is tracked apart
 * from `variables` because the child environment starts from the server's own, so an unset name
 * would keep an inherited credential.
 */
export interface ResolvedProviderInstanceEnvironment {
  readonly variables: ResolvedProviderEnvironment;
  readonly unresolved: ReadonlyArray<string>;
}

export interface ProviderSecretResolverShape {
  /**
   * Replace every named secret with its value; literals pass through. Never fails: an unreadable
   * reference is reported unresolved, never as an empty string, and the registry unsets its name
   * so an inherited same-named credential is not used.
   */
  readonly resolve: (
    environment: ProviderInstanceEnvironment | undefined,
  ) => Effect.Effect<ResolvedProviderInstanceEnvironment>;
  /**
   * Resolve `references` ahead of callers in as few store invocations as possible, since unlocking
   * is charged per invocation. Best effort and never fails: a failed batch leaves the cache cold.
   */
  readonly prime: (references: ReadonlyArray<ProviderSecretReference>) => Effect.Effect<void>;
  /**
   * Drop cached secrets so the next `resolve` re-reads them: all by default, or only those from
   * the named providers. Running providers keep their spawn-time environment until rebuilt.
   */
  readonly invalidate: (providers?: ReadonlySet<SecretProviderId>) => Effect.Effect<void>;
  /**
   * Read one secret source fresh for the Settings "check" button. A success is cached; never
   * fails and never returns the value.
   */
  readonly check: (source: ProviderSecretSource) => Effect.Effect<SecretSourceCheckResult>;
  /**
   * The 1Password accounts signed in on this machine, so settings can offer
   * them instead of asking the user to type one. Reads local `op` config only,
   * so it never prompts. Empty when `op` is missing or fails.
   */
  readonly listOnePasswordAccounts: Effect.Effect<ReadonlyArray<OnePasswordAccountSummary>>;
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
  "t3/provider/ProviderSecretResolver",
  {
    defaultValue: () => ({
      resolve: (environment) => Effect.sync(() => resolveWithoutSecretStore(environment)),
      prime: () => Effect.void,
      invalidate: () => Effect.void,
      check: () =>
        Effect.succeed({
          _tag: "Failed",
          detail: "Secret stores are not available on this server",
        } as const),
      listOnePasswordAccounts: Effect.succeed([]),
    }),
  },
) {}

export class SettingsSecretUnresolvedError extends Schema.TaggedError<SettingsSecretUnresolvedError>()(
  "SettingsSecretUnresolvedError",
  { provider: SecretProviderId },
) {
  override get message(): string {
    return `Could not read the credential from the secret provider "${this.provider}". Unlock or fix it in Settings, then try again.`;
  }
}

/**
 * The secret a credential setting holds or names, through the resolver's cache. A named secret
 * that cannot be read, or reads empty, fails instead of passing for a missing token, so callers
 * that would fall back to another credential cannot silently switch accounts.
 */
export const requireSettingsSecret = (
  resolver: ProviderSecretResolverShape,
  value: SettingsSecretValue,
): Effect.Effect<string, SettingsSecretUnresolvedError> =>
  typeof value === "string"
    ? Effect.succeed(value)
    : resolver.resolve([{ name: "T3_SETTINGS_SECRET", value, sensitive: true }]).pipe(
        Effect.flatMap(({ variables }) => {
          const secret = variables[0]?.value ?? "";
          return secret.trim() === ""
            ? Effect.fail(new SettingsSecretUnresolvedError({ provider: value.provider }))
            : Effect.succeed(secret);
        }),
      );

/**
 * Like `requireSettingsSecret`, but an unreadable secret comes back as `""`, so callers treat it
 * exactly like a missing token. Only for credentials whose documented fallback is acceptable.
 */
export const resolveSettingsSecret = (
  resolver: ProviderSecretResolverShape,
  value: SettingsSecretValue,
): Effect.Effect<string> =>
  requireSettingsSecret(resolver, value).pipe(
    Effect.catchTags({ SettingsSecretUnresolvedError: () => Effect.succeed("") }),
  );
