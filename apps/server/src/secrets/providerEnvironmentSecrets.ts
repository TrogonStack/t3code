import type { ProviderInstanceEnvironment, SecretReference } from "@t3tools/contracts";
import type * as SecretResolver from "@t3tools/secret-provider-core/server/SecretResolver";
import * as Effect from "effect/Effect";

export interface ProviderEnvironmentSecrets {
  /** Literal variables plus every secret that was read, ready to overlay on a process environment. */
  readonly environment: ReadonlyArray<{
    readonly name: string;
    readonly value: string;
    readonly sensitive?: boolean | undefined;
  }>;
  /** Each secret that could not be read or read empty. Values are never part of it. */
  readonly unreadable: ReadonlyArray<{
    readonly name: string;
    readonly reference: SecretReference;
  }>;
}

/**
 * Reads the secret-backed variables of a provider instance, so a caller never launches or probes
 * with a credential silently missing. `only` limits the read to the named variables, for callers
 * that need a few of them and should not fail on an unrelated secret.
 */
export const resolveProviderEnvironmentSecrets = Effect.fn("ProviderEnvironmentSecrets.resolve")(
  function* (
    secrets: SecretResolver.SecretResolver["Service"] | undefined,
    environment: ProviderInstanceEnvironment | undefined,
    only?: ReadonlyArray<string>,
  ) {
    const secretVariables = (environment ?? []).flatMap((variable) =>
      typeof variable.value === "string" || (only !== undefined && !only.includes(variable.name))
        ? []
        : [{ name: variable.name, reference: variable.value }],
    );
    const values =
      secretVariables.length === 0
        ? []
        : secrets === undefined
          ? secretVariables.map(() => undefined)
          : yield* secrets.read(secretVariables.map(({ reference }) => reference));
    const unreadable = secretVariables.filter((_, index) => (values[index] ?? "").trim() === "");
    let secretIndex = 0;
    return {
      environment: (environment ?? []).flatMap(({ name, value, sensitive }) => {
        if (typeof value === "string") return [{ name, value, sensitive }];
        if (only !== undefined && !only.includes(name)) return [];
        const read = values[secretIndex++];
        return read === undefined || read.trim() === ""
          ? []
          : [{ name, value: read, sensitive: true }];
      }),
      unreadable,
    } satisfies ProviderEnvironmentSecrets;
  },
);
