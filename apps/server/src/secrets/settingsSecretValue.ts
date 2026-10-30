import {
  isSecretReference,
  secretReferencesInSettings,
  type SettingsSecretValue,
} from "@t3tools/contracts";
import type * as SecretResolverCore from "@t3tools/secret-provider-core/server/SecretResolver";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

type SecretResolverShape = SecretResolverCore.SecretResolver["Service"];

export class SettingsSecretUnresolvedError extends Schema.TaggedError<SettingsSecretUnresolvedError>()(
  "SettingsSecretUnresolvedError",
  { provider: Schema.String },
) {
  override get message(): string {
    return `Could not read the credential from the secret provider "${this.provider}". Unlock or fix it in Settings, then try again.`;
  }
}

/**
 * The secret a credential setting holds or names. A named secret that cannot be read, or reads
 * empty, fails instead of passing for a missing token, so callers that would fall back to another
 * credential cannot silently switch accounts.
 */
export const requireSettingsSecret = (
  resolver: SecretResolverShape,
  value: SettingsSecretValue,
): Effect.Effect<string, SettingsSecretUnresolvedError> =>
  !isSecretReference(value)
    ? Effect.succeed(value)
    : resolver
        .read([value])
        .pipe(
          Effect.flatMap(([secret]) =>
            secret === undefined || secret.trim() === ""
              ? Effect.fail(new SettingsSecretUnresolvedError({ provider: value.provider }))
              : Effect.succeed(secret),
          ),
        );

/** An unreadable secret comes back as `""`, for credentials whose documented fallback is acceptable. */
export const resolveSettingsSecret = (
  resolver: SecretResolverShape,
  value: SettingsSecretValue,
): Effect.Effect<string> =>
  requireSettingsSecret(resolver, value).pipe(
    Effect.catchTags({ SettingsSecretUnresolvedError: () => Effect.succeed("") }),
  );

/**
 * Replaces each reference in the fields `schema` declares as `SettingsSecretValue` with the secret
 * it names, read in one batch. An unreadable secret comes back as `""`, so it behaves as unset.
 */
export const resolveSettingsSecrets = Effect.fn("SettingsSecrets.resolve")(function* <
  Settings extends Readonly<Record<string, unknown>>,
>(resolver: SecretResolverShape, schema: Schema.Top, settings: Settings) {
  const pending = secretReferencesInSettings(schema, settings);
  if (pending.length === 0) return settings;
  const values = yield* resolver.read(pending.map(({ reference }) => reference));
  const resolved = Object.fromEntries(
    pending.map(({ field }, index) => [field, values[index] ?? ""] as const),
  );
  return { ...settings, ...resolved };
});
