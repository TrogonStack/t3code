import type {
  ProviderInstanceEnvironment,
  ProviderInstanceEnvironmentVariableName,
} from "@t3tools/contracts";
import * as HostProcess from "@t3tools/shared/HostProcess";
import * as Effect from "effect/Effect";

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

const RESOURCE_ATTRIBUTES = "OTEL_RESOURCE_ATTRIBUTES";

/**
 * Marks the telemetry of every process T3 Code launches, so a harness such as
 * Claude Code or Codex reports which of its sessions T3 Code drove. A value the
 * user already set for the key wins.
 */
export const LAUNCHED_BY_ATTRIBUTE = { key: "t3code.launched_by", value: "t3code" } as const;

const hasResourceAttribute = (attributes: string, key: string): boolean =>
  attributes.split(",").some((entry) => {
    const name = entry.split("=", 1)[0]?.trim() ?? "";
    try {
      return decodeURIComponent(name) === key;
    } catch {
      return name === key;
    }
  });

const withLaunchedByAttribute = (environment: NodeJS.ProcessEnv): NodeJS.ProcessEnv => {
  const attributes = environment[RESOURCE_ATTRIBUTES]?.trim() ?? "";
  if (hasResourceAttribute(attributes, LAUNCHED_BY_ATTRIBUTE.key)) {
    return environment;
  }
  const entry = `${encodeURIComponent(LAUNCHED_BY_ATTRIBUTE.key)}=${encodeURIComponent(LAUNCHED_BY_ATTRIBUTE.value)}`;
  return {
    ...environment,
    [RESOURCE_ATTRIBUTES]: attributes === "" ? entry : `${attributes},${entry}`,
  };
};

/**
 * Lays the configured variables over `baseEnv` without marking the result as
 * launched by T3 Code. Callers whose `baseEnv` is only a set of overrides use
 * this, since adding `OTEL_RESOURCE_ATTRIBUTES` there would replace the
 * inherited value instead of extending it.
 */
export const overlayProviderInstanceEnvironment = Effect.fn(function* (
  environment: ResolvedProviderEnvironment | undefined,
  baseEnv: NodeJS.ProcessEnv,
) {
  if (!environment || environment.length === 0) {
    return baseEnv;
  }

  const home = yield* HostProcess.HomeDirectory;
  const next: NodeJS.ProcessEnv = { ...baseEnv };
  for (const variable of environment) {
    // Child processes do not apply shell expansion to environment values.
    next[variable.name] =
      variable.name === "CODEX_HOME" || variable.name === "CLAUDE_CONFIG_DIR"
        ? expandHomePath(variable.value, home)
        : variable.value;
  }
  return next;
});

export const mergeProviderInstanceEnvironment = Effect.fn(function* (
  environment: ResolvedProviderEnvironment | undefined,
  baseEnv: NodeJS.ProcessEnv = process.env,
) {
  return withLaunchedByAttribute(yield* overlayProviderInstanceEnvironment(environment, baseEnv));
});
