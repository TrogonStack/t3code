import type { ProviderInstanceEnvironment } from "@t3tools/contracts";
import { literalProviderInstanceEnvironment } from "@t3tools/provider-core/server/instanceEnvironment";
import type * as SecretResolver from "@t3tools/secret-provider-core/server/SecretResolver";
import * as Effect from "effect/Effect";

/**
 * Variables that place a provider's session history on disk. Readers that only look for a
 * transcript directory need these resolved, and nothing else.
 */
export const PROVIDER_HOME_ENVIRONMENT_NAMES: ReadonlyArray<string> = [
  "CLAUDE_CONFIG_DIR",
  "CODEX_HOME",
  "ANTIGRAVITY_DATA_DIR",
  "PI_CODING_AGENT_DIR",
  "PI_CODING_AGENT_SESSION_DIR",
];

export interface ProviderEnvironmentSecrets {
  /** Literal variables plus every secret that was read, ready to overlay on a process environment. */
  readonly environment: ReadonlyArray<{
    readonly name: string;
    readonly value: string;
    readonly sensitive?: boolean | undefined;
  }>;
  /** `NAME (provider:reference)` for each secret that could not be read or read empty. */
  readonly unreadable: ReadonlyArray<string>;
}

/**
 * Reads the secret-backed variables of a provider instance, so a caller never launches or probes
 * with a credential silently missing. `only` limits the read to the named variables, for callers
 * that need a few of them and should not fail on an unrelated secret. Values are never part of
 * `unreadable`.
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
    return {
      environment: [
        ...literalProviderInstanceEnvironment(environment),
        ...secretVariables.flatMap(({ name }, index) => {
          const value = values[index];
          return value === undefined || value.trim() === ""
            ? []
            : [{ name, value, sensitive: true }];
        }),
      ],
      unreadable: unreadable.map(
        ({ name, reference }) => `${name} (${reference.provider}:${reference.reference})`,
      ),
    } satisfies ProviderEnvironmentSecrets;
  },
);
