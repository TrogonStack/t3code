import type {
  ProviderInstanceEnvironment,
  ProviderInstanceEnvironmentVariableName,
} from "@t3tools/contracts";

import { expandHomePath } from "./pathExpansion.ts";

/**
 * A provider environment variable whose value is ready for a child process.
 * Configured variables may name a secret source instead of a value; only
 * `ProviderSecretResolver` turns those into this shape, so a source cannot
 * reach a process by accident. A value read from a secret store is always
 * sensitive.
 */
export interface ResolvedProviderEnvironmentVariable {
  readonly name: ProviderInstanceEnvironmentVariableName;
  readonly value: string;
  readonly sensitive: boolean;
}

export type ResolvedProviderEnvironment = ReadonlyArray<ResolvedProviderEnvironmentVariable>;

/**
 * The configured variables that hold literal values, for callers that build a
 * process environment without resolving secrets. Variables that read from a
 * secret store are left out.
 */
export function literalProviderInstanceEnvironment(
  environment: ProviderInstanceEnvironment | undefined,
): ResolvedProviderEnvironment {
  const literals: Array<ResolvedProviderEnvironmentVariable> = [];
  for (const { name, value, sensitive } of environment ?? []) {
    if (typeof value === "string") {
      literals.push({ name, value, sensitive });
    }
  }
  return literals;
}

export function mergeProviderInstanceEnvironment(
  environment: ResolvedProviderEnvironment | undefined,
  baseEnv: NodeJS.ProcessEnv = process.env,
): NodeJS.ProcessEnv {
  if (!environment || environment.length === 0) {
    return baseEnv;
  }

  const next: NodeJS.ProcessEnv = { ...baseEnv };
  for (const variable of environment) {
    // Child processes do not apply shell expansion to environment values.
    next[variable.name] =
      variable.name === "CODEX_HOME" || variable.name === "CLAUDE_CONFIG_DIR"
        ? expandHomePath(variable.value)
        : variable.value;
  }
  return next;
}
