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

export const RESOURCE_ATTRIBUTES = "OTEL_RESOURCE_ATTRIBUTES";

/**
 * Marks the telemetry of every process T3 Code launches, so a harness such as
 * Claude Code or Codex reports which of its sessions T3 Code drove. A value the
 * user already set for the key wins.
 */
const LAUNCHED_ATTRIBUTE = { key: "t3code.launched", value: "true" } as const;

const resourceAttributeValue = (attributes: string, key: string): string | undefined => {
  for (const entry of attributes.split(",")) {
    const separator = entry.indexOf("=");
    const rawName = (separator === -1 ? entry : entry.slice(0, separator)).trim();
    const rawValue = separator === -1 ? "" : entry.slice(separator + 1).trim();
    try {
      if (decodeURIComponent(rawName) === key) {
        return decodeURIComponent(rawValue);
      }
    } catch {
      if (rawName === key) {
        return rawValue;
      }
    }
  }
  return undefined;
};

/**
 * The value the user set for `t3code.launched` in a resource attribute list,
 * or `undefined` when the list does not carry it.
 */
export const userLaunchedAttribute = (attributes: string): string | undefined =>
  resourceAttributeValue(attributes, LAUNCHED_ATTRIBUTE.key);

export const withLaunchedAttribute = (attributes: string): string => {
  const trimmed = attributes.trim();
  if (userLaunchedAttribute(trimmed) !== undefined) {
    return trimmed;
  }
  const entry = `${encodeURIComponent(LAUNCHED_ATTRIBUTE.key)}=${encodeURIComponent(LAUNCHED_ATTRIBUTE.value)}`;
  return trimmed === "" ? entry : `${trimmed},${entry}`;
};

const withLaunchedByAttribute = (environment: NodeJS.ProcessEnv): NodeJS.ProcessEnv => {
  const attributes = environment[RESOURCE_ATTRIBUTES]?.trim() ?? "";
  if (userLaunchedAttribute(attributes) !== undefined) {
    return environment;
  }
  return { ...environment, [RESOURCE_ATTRIBUTES]: withLaunchedAttribute(attributes) };
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
