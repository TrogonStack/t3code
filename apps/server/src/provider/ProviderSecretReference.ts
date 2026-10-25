/**
 * Provider environment values that name a secret (`secret-provider` or inline `1password`)
 * instead of carrying one. Pure helpers, so the registry need not depend on the resolver.
 * @module provider/ProviderSecretReference
 */
import type {
  OnePasswordAccount,
  ProviderInstanceEnvironment,
  ProviderInstanceEnvironmentVariable,
  SecretProviderId,
} from "@t3tools/contracts";
import * as Data from "effect/Data";
import * as Equal from "effect/Equal";

/**
 * One secret to read from 1Password. Structurally comparable, so it doubles as
 * the resolver's cache key: the same reference in two accounts is two secrets.
 */
export class OnePasswordSecretReference extends Data.TaggedClass("1password")<{
  readonly reference: string;
  readonly account: OnePasswordAccount;
}> {}

/**
 * One secret to read through a named provider. The provider id is part of the
 * cache key, so the same reference behind two providers is two secrets. What
 * the reference means depends on the provider's kind, which is looked up when
 * the secret is read.
 */
export class NamedProviderSecretReference extends Data.TaggedClass("secret-provider")<{
  readonly provider: SecretProviderId;
  readonly reference: string;
}> {}

/** Every secret a provider environment value can name. */
export type ProviderSecretReference = OnePasswordSecretReference | NamedProviderSecretReference;

/**
 * The secret an environment value names, or `undefined` when the value is a
 * literal.
 */
export function providerSecretReference(
  value: ProviderInstanceEnvironmentVariable["value"],
): ProviderSecretReference | undefined {
  if (typeof value === "string") {
    return undefined;
  }
  switch (value.kind) {
    case "1password":
      return new OnePasswordSecretReference({
        reference: value.reference,
        account: value.account,
      });
    case "secret-provider":
      return new NamedProviderSecretReference({
        provider: value.provider,
        reference: value.reference,
      });
  }
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
 * Every distinct secret across a set of environments, in first-seen order. Unlocking is per `op`
 * invocation, so callers building many instances want the whole list up front.
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
