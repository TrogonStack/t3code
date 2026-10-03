/**
 * Provider-instance contracts.
 *
 * Splits the historical "provider kind" concept into two:
 *
 *   - `ProviderDriverKind` is the implementation kind selector (e.g. codex,
 *     claudeAgent, a fork's `ollama`, …). It picks which driver package
 *     handles the protocol, the probe, the adapter, and text generation.
 *
 *   - `ProviderInstanceId` is the routing key (a user-defined slug).
 *     Threads, sessions, runtime events, and persisted bindings reference
 *     instance ids — never driver kinds — so a user can configure multiple
 *     instances of the same driver (e.g. `codex_personal` + `codex_work`),
 *     each with independent driver-specific configuration.
 *
 * Forward/backward compatibility invariant
 * ----------------------------------------
 * `ProviderDriverKind` is intentionally an **open** branded slug, not a closed
 * literal union. The server hosts forks, ships in PRs that add drivers, and
 * users frequently roll between branches and forks. Any of those paths can
 * leave `ServerSettings`, persisted thread state, or session bindings
 * referencing a driver that the currently-running build does not know about.
 *
 * The rule: parsing any of those payloads must always succeed, and the
 * runtime is responsible for marking the unknown driver/instance as
 * "unavailable" rather than crashing. Built-in drivers shipped by the core
 * product happens to register in a given build is not part of the contract
 * layer. Driver availability is discovered through the runtime registry.
 *
 * Driver-specific configuration is similarly opaque at the contracts layer:
 * drivers live in (or will be extracted to) their own packages and own their
 * config schemas. The contracts package only knows the envelope.
 *
 * @module providerInstance
 */
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import { TrimmedNonEmptyString } from "./baseSchemas.ts";

const PROVIDER_SLUG_MAX_CHARS = 64;
/**
 * Slug pattern shared by driver kinds and instance ids — letters, digits,
 * dashes, underscores. The first character must be a letter so slugs remain
 * JS-identifier friendly when used as object keys, log fields, or telemetry
 * attributes. Mixed case is permitted so historical driver kinds (e.g.
 * `claudeAgent`) can be used verbatim during the migration and so external
 * fork authors retain reasonable freedom.
 */
const PROVIDER_SLUG_PATTERN = /^[a-zA-Z][a-zA-Z0-9_-]*$/;
const ENVIRONMENT_VARIABLE_NAME_MAX_CHARS = 128;
const ENVIRONMENT_VARIABLE_NAME_PATTERN = /^[a-zA-Z_][a-zA-Z0-9_]*$/;

const slugSchema = TrimmedNonEmptyString.check(
  Schema.isMaxLength(PROVIDER_SLUG_MAX_CHARS),
  Schema.isPattern(PROVIDER_SLUG_PATTERN),
);

/**
 * `ProviderDriverKind` — open branded slug naming a driver implementation.
 *
 * Constraints (validated at the schema layer):
 *   - starts with a letter
 *   - only letters, digits, `-`, `_` after the first char
 *   - 1..64 characters
 *
 * Notably **not** validated: that the driver is one we know how to load.
 * That check belongs to the runtime registry, which downgrades unknown
 * drivers gracefully (see module docs).
 */
export const ProviderDriverKind = slugSchema.pipe(Schema.brand("ProviderDriverKind"));
export type ProviderDriverKind = typeof ProviderDriverKind.Type;

const isProviderDriverKindValue = Schema.is(ProviderDriverKind);
export const isProviderDriverKind = (value: unknown): value is ProviderDriverKind =>
  isProviderDriverKindValue(value);

/**
 * `ProviderInstanceId` — user-defined routing key for a configured provider
 * instance. Same slug rules as `ProviderDriverKind`; branded separately so the
 * type system cannot confuse the two.
 */
export const ProviderInstanceId = slugSchema.pipe(Schema.brand("ProviderInstanceId"));
export type ProviderInstanceId = typeof ProviderInstanceId.Type;

/**
 * Lightweight reference identifying which driver implements an instance.
 * Carried alongside `ProviderInstanceId` on wire shapes so consumers can
 * branch on driver behavior (icons, capabilities, presentation) without
 * having to look up the instance in the registry.
 */
export const ProviderInstanceRef = Schema.Struct({
  instanceId: ProviderInstanceId,
  driver: ProviderDriverKind,
});
export type ProviderInstanceRef = typeof ProviderInstanceRef.Type;

export const ProviderInstanceEnvironmentVariableName = TrimmedNonEmptyString.check(
  Schema.isMaxLength(ENVIRONMENT_VARIABLE_NAME_MAX_CHARS),
  Schema.isPattern(ENVIRONMENT_VARIABLE_NAME_PATTERN),
);
export type ProviderInstanceEnvironmentVariableName =
  typeof ProviderInstanceEnvironmentVariableName.Type;

const ONE_PASSWORD_SECRET_REFERENCE_PREFIX = "op://";
const ONE_PASSWORD_SECRET_REFERENCE_MAX_CHARS = 1024;
const ONE_PASSWORD_SECRET_REFERENCE_QUERY_PATTERN = /^[A-Za-z0-9._=&-]+$/;
const ONE_PASSWORD_ACCOUNT_MAX_CHARS = 253;
const ONE_PASSWORD_ACCOUNT_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

function onePasswordSecretReferenceIssue(value: string): string | undefined {
  if (value !== value.trim()) {
    return "1Password secret reference must not start or end with whitespace.";
  }
  if (hasControlCharacter(value)) {
    return "1Password secret reference must not contain control characters.";
  }
  if (!value.startsWith(ONE_PASSWORD_SECRET_REFERENCE_PREFIX)) {
    return "1Password secret reference must start with op://.";
  }
  // The resolver embeds references in an `op inject` template, where `{{` and
  // `}}` delimit a reference.
  if (value.includes("{{") || value.includes("}}")) {
    return "1Password secret reference must not contain {{ or }}.";
  }
  const [path = "", query, ...extraQueries] = value
    .slice(ONE_PASSWORD_SECRET_REFERENCE_PREFIX.length)
    .split("?");
  if (
    extraQueries.length > 0 ||
    (query !== undefined && !ONE_PASSWORD_SECRET_REFERENCE_QUERY_PATTERN.test(query))
  ) {
    return "1Password secret reference has an invalid query.";
  }
  const segments = path.split("/");
  if (
    segments.length < 3 ||
    segments.length > 4 ||
    segments.some((segment) => segment.trim().length === 0)
  ) {
    return "1Password secret reference must look like op://vault/item/field or op://vault/item/section/field.";
  }
  return undefined;
}

/**
 * A 1Password secret reference, `op://vault/item/[section/]field`, optionally
 * followed by a `?query` such as `?attribute=otp`. Names may contain spaces.
 */
export const OnePasswordSecretReference = Schema.String.check(
  Schema.isMaxLength(ONE_PASSWORD_SECRET_REFERENCE_MAX_CHARS),
  Schema.makeFilter((value: string) => onePasswordSecretReferenceIssue(value) ?? true),
).pipe(Schema.brand("OnePasswordSecretReference"));
export type OnePasswordSecretReference = typeof OnePasswordSecretReference.Type;

/**
 * The 1Password account a reference is read from, in any form `op --account`
 * accepts: account shorthand, sign-in address (`my.1password.com`), account ID,
 * or user ID. It is passed to the CLI as an argument, so it can never start
 * with `-`.
 */
export const OnePasswordAccount = Schema.String.check(
  Schema.isMaxLength(ONE_PASSWORD_ACCOUNT_MAX_CHARS),
  Schema.makeFilter((value: string) =>
    ONE_PASSWORD_ACCOUNT_PATTERN.test(value)
      ? true
      : "1Password account must be a shorthand, sign-in address, or ID with no spaces.",
  ),
).pipe(Schema.brand("OnePasswordAccount"));
export type OnePasswordAccount = typeof OnePasswordAccount.Type;

export const OnePasswordSecretSource = Schema.Struct({
  kind: Schema.Literal("1password"),
  reference: OnePasswordSecretReference,
  account: OnePasswordAccount,
});
export type OnePasswordSecretSource = typeof OnePasswordSecretSource.Type;

/**
 * An environment value read from an external secret store at the moment the
 * provider process starts, instead of a literal. Discriminated on `kind`.
 */
export const ProviderSecretSource = Schema.Union([OnePasswordSecretSource]);
export type ProviderSecretSource = typeof ProviderSecretSource.Type;

/**
 * `sensitive` and `valueRedacted` only describe literal string values; a
 * secret source is not itself a secret and is stored as written.
 */
export const ProviderInstanceEnvironmentVariable = Schema.Struct({
  name: ProviderInstanceEnvironmentVariableName,
  value: Schema.Union([Schema.String, ProviderSecretSource]).pipe(
    Schema.withDecodingDefault(Effect.succeed("")),
  ),
  sensitive: Schema.Boolean.pipe(Schema.withDecodingDefault(Effect.succeed(false))),
  valueRedacted: Schema.optionalKey(Schema.Boolean),
});
export type ProviderInstanceEnvironmentVariable = typeof ProviderInstanceEnvironmentVariable.Type;

export const ProviderInstanceEnvironment = Schema.Array(ProviderInstanceEnvironmentVariable);
export type ProviderInstanceEnvironment = typeof ProviderInstanceEnvironment.Type;

/**
 * Envelope shape for a provider instance configuration in `ServerSettings`.
 *
 * `driver` is intentionally accepted as any well-formed slug (see module
 * docs). The driver-specific config payload is left as `Schema.Unknown`;
 * each driver registers its own decoder with the runtime registry, and
 * envelopes for unknown drivers are preserved verbatim so they round-trip
 * across version changes without data loss.
 */
export const ProviderInstanceConfig = Schema.Struct({
  driver: ProviderDriverKind,
  displayName: Schema.optional(TrimmedNonEmptyString),
  accentColor: Schema.optional(TrimmedNonEmptyString),
  environment: Schema.optionalKey(ProviderInstanceEnvironment),
  enabled: Schema.optionalKey(Schema.Boolean),
  config: Schema.optionalKey(Schema.Unknown),
});
export type ProviderInstanceConfig = typeof ProviderInstanceConfig.Type;

/** Atomic mutation for one provider-instance map entry. */
export const ProviderInstanceMutation = Schema.Union([
  Schema.Struct({
    operation: Schema.Literal("create"),
    instanceId: ProviderInstanceId,
    instance: ProviderInstanceConfig,
  }),
  Schema.Struct({
    operation: Schema.Literal("upsert"),
    instanceId: ProviderInstanceId,
    instance: ProviderInstanceConfig,
  }),
  Schema.Struct({
    operation: Schema.Literal("remove"),
    instanceId: ProviderInstanceId,
  }),
]);
export type ProviderInstanceMutation = typeof ProviderInstanceMutation.Type;

/**
 * Map shape for `ServerSettings.providerInstances`. Keyed by
 * `ProviderInstanceId`, values are envelopes the registry feeds to drivers.
 */
export const ProviderInstanceConfigMap = Schema.Record(ProviderInstanceId, ProviderInstanceConfig);
export type ProviderInstanceConfigMap = typeof ProviderInstanceConfigMap.Type;

/**
 * Construct the canonical `ProviderInstanceId` used as a back-compat default
 * for a built-in driver. The legacy single-instance-per-driver world used
 * the driver kind itself as the instance id; preserving that mapping keeps
 * existing persisted threads, bindings, and cache files routable across the
 * migration without rewriting their stored selection payloads.
 */
export const defaultInstanceIdForDriver = (driver: ProviderDriverKind): ProviderInstanceId =>
  ProviderInstanceId.make(driver);
