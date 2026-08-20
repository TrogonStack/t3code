/**
 * Provider environment values that name a secret instead of carrying one.
 *
 * A user who keeps a provider credential in 1Password can paste the item's
 * secret reference (`op://Vault/Item/field`) as an environment variable's
 * value instead of the secret itself. `ProviderSecretResolver` swaps the
 * reference for the real value on the way into the provider process, so the
 * credential never lands in `settings.json` or the on-disk secret store, and
 * rotating it in 1Password rotates it here.
 *
 * These helpers are pure so the instance registry can ask "does this
 * environment read from a secret store?" without depending on the resolver.
 *
 * @module provider/ProviderSecretReference
 */
import type { ProviderInstanceEnvironment } from "@t3tools/contracts";

/** URI scheme 1Password uses for secret references; `op read` consumes these. */
export const PROVIDER_SECRET_REFERENCE_PREFIX = "op://";

/**
 * The secret reference an environment value names, or `undefined` when the
 * value is a literal. The value is trimmed first: a reference copied out of a
 * password manager routinely arrives with surrounding whitespace, which `op`
 * rejects.
 */
export function providerSecretReference(value: string): string | undefined {
  const trimmed = value.trim();
  if (!trimmed.startsWith(PROVIDER_SECRET_REFERENCE_PREFIX)) {
    return undefined;
  }
  return trimmed.length > PROVIDER_SECRET_REFERENCE_PREFIX.length ? trimmed : undefined;
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
 * Every distinct secret reference across a set of environments, in the order
 * they were first seen.
 *
 * Resolution is per reference but unlocking is per `op` invocation, so the
 * caller that is about to build many instances wants the whole list up front:
 * one call covering every reference costs one authorization, where one call
 * per instance costs one each.
 */
export function collectProviderSecretReferences(
  environments: Iterable<ProviderInstanceEnvironment | undefined>,
): ReadonlyArray<string> {
  const references = new Set<string>();
  for (const environment of environments) {
    for (const variable of environment ?? []) {
      const reference = providerSecretReference(variable.value);
      if (reference !== undefined) {
        references.add(reference);
      }
    }
  }
  return Array.from(references);
}
