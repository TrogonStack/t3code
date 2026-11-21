import type { ProviderInstanceEnvironment } from "@t3tools/contracts";
import type * as SecretResolver from "@t3tools/secret-provider-core/server/SecretResolver";
import * as Effect from "effect/Effect";

const PROVIDER_HOME_ENVIRONMENT_NAMES = new Map<string, ReadonlyArray<string>>([
  ["claudeAgent", ["CLAUDE_CONFIG_DIR"]],
  ["codex", ["CODEX_HOME"]],
  ["antigravity", ["ANTIGRAVITY_DATA_DIR"]],
  ["pi", ["PI_CODING_AGENT_DIR", "PI_CODING_AGENT_SESSION_DIR"]],
]);

/**
 * Variables that place a driver's session history on disk. Readers that only look for a
 * transcript directory need these resolved, and nothing else.
 */
export const providerHomeEnvironmentNames = (driver: string): ReadonlyArray<string> =>
  PROVIDER_HOME_ENVIRONMENT_NAMES.get(driver) ?? [];

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
      unreadable: unreadable.map(
        ({ name, reference }) => `${name} (${reference.provider}:${reference.reference})`,
      ),
    } satisfies ProviderEnvironmentSecrets;
  },
);
