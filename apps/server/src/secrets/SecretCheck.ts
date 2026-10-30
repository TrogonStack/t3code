import type { SecretReference, SecretSourceCheckResult } from "@t3tools/contracts";
import * as SecretResolver from "@t3tools/secret-provider-core/server/SecretResolver";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import * as ProviderRegistry from "../provider/ProviderRegistry.ts";

export class SecretCheck extends Context.Service<
  SecretCheck,
  {
    /**
     * Reads one secret fresh for the Settings "check" button and never returns the value. A
     * readable secret also rebuilds the provider instances that depend on a changed value.
     */
    readonly check: (reference: SecretReference) => Effect.Effect<SecretSourceCheckResult>;
  }
>()("t3/secrets/SecretCheck") {}

export const layer = Layer.effect(
  SecretCheck,
  Effect.gen(function* () {
    const resolver = yield* SecretResolver.SecretResolver;
    const providerRegistry = yield* ProviderRegistry.ProviderRegistry;

    const check = Effect.fn("SecretCheck.check")(function* (reference: SecretReference) {
      yield* resolver.invalidate([reference]);
      const [value] = yield* resolver.read([reference]);
      if (value === undefined) {
        return {
          _tag: "Failed",
          detail: `Could not read this secret from "${reference.provider}". Check the provider's settings and that its store is unlocked.`,
        } as const;
      }
      yield* providerRegistry.reloadSecretsAndRefresh();
      return { _tag: "Resolved" } as const;
    });

    return SecretCheck.of({ check });
  }),
);
