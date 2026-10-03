/**
 * Provider environment values that name a secret instead of carrying one.
 *
 * A user who keeps a provider credential in 1Password configures the variable
 * with a 1Password secret source (`{ kind: "1password", reference, account }`)
 * instead of the secret itself. `ProviderSecretResolver` swaps the source for
 * the real value on the way into the provider process, so the credential never
 * lands in `settings.json` or the on-disk secret store, and rotating it in
 * 1Password rotates it here.
 *
 * These helpers are pure so the instance registry can ask "does this
 * environment read from a secret store?" without depending on the resolver.
 *
 * @module provider/ProviderSecretReference
 */
import type {
  OnePasswordAccount,
  ProviderInstanceEnvironment,
  ProviderInstanceEnvironmentVariable,
} from "@t3tools/contracts";
import * as Data from "effect/Data";
import * as Equal from "effect/Equal";

/** URI scheme 1Password uses for secret references; `op read` consumes these. */
const ONE_PASSWORD_SECRET_REFERENCE_PREFIX = "op://";

/**
 * One secret to read from 1Password. Structurally comparable, so it doubles as
 * the resolver's cache key: the same reference in two accounts is two secrets.
 * `account` is absent only for legacy plain-string references, which `op`
 * reads from its default account.
 */
export class OnePasswordSecretReference extends Data.TaggedClass("1password")<{
  readonly reference: string;
  readonly account: OnePasswordAccount | undefined;
}> {}

/** Every secret a provider environment value can name. */
export type ProviderSecretReference = OnePasswordSecretReference;

/**
 * The secret an environment value names, or `undefined` when the value is a
 * literal.
 */
export function providerSecretReference(
  value: ProviderInstanceEnvironmentVariable["value"],
): ProviderSecretReference | undefined {
  if (typeof value !== "string") {
    switch (value.kind) {
      case "1password":
        return new OnePasswordSecretReference({
          reference: value.reference,
          account: value.account,
        });
    }
  }
  // Legacy: a plain string beginning with `op://` predates secret sources and
  // is read from the default `op` account. Trimmed because a reference copied
  // out of a password manager routinely arrives with surrounding whitespace.
  const trimmed = value.trim();
  if (
    !trimmed.startsWith(ONE_PASSWORD_SECRET_REFERENCE_PREFIX) ||
    trimmed.length === ONE_PASSWORD_SECRET_REFERENCE_PREFIX.length
  ) {
    return undefined;
  }
  return new OnePasswordSecretReference({ reference: trimmed, account: undefined });
}

/**
 * Whether any variable in the environment reads from a secret store. Drives
 * the "rebuild this instance on refresh" decision, so instances configured
 * entirely with literals keep the process they already have.
 */
export function hasProviderSecretReference(
  environment: ProviderInstanceEnvironment | undefined,
): boolean {
  return (
    environment?.some((variable) => providerSecretReference(variable.value) !== undefined) ?? false
  );
}

/**
 * Every distinct secret across a set of environments, in the order they were
 * first seen.
 *
 * Resolution is per secret but unlocking is per `op` invocation, so the caller
 * that is about to build many instances wants the whole list up front: one
 * call covering every secret costs one authorization, where one call per
 * instance costs one each.
 */
export function collectProviderSecretReferences(
  environments: Iterable<ProviderInstanceEnvironment | undefined>,
): ReadonlyArray<ProviderSecretReference> {
  const references: Array<ProviderSecretReference> = [];
  for (const environment of environments) {
    for (const variable of environment ?? []) {
      const reference = providerSecretReference(variable.value);
      if (reference !== undefined && !references.some((seen) => Equal.equals(seen, reference))) {
        references.push(reference);
      }
    }
  }
  return references;
}
