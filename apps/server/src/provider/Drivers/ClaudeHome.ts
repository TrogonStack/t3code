import type { ClaudeSettings } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";

import {
  RESOURCE_ATTRIBUTES,
  userLaunchedAttribute,
  withLaunchedAttribute,
} from "@t3tools/provider-core/server/instanceEnvironment";
import { expandHomePath } from "@t3tools/provider-core/server/pathExpansion";
import * as HostProcess from "@t3tools/shared/HostProcess";
import { fromLenientJson } from "@t3tools/shared/schemaJson";

const quotePath = Schema.encodeSync(Schema.fromJsonString(Schema.String));

export const resolveClaudeHomePath = Effect.fn("resolveClaudeHomePath")(function* (
  config: Pick<ClaudeSettings, "homePath">,
  environment?: NodeJS.ProcessEnv,
): Effect.fn.Return<string, never, Path.Path> {
  const path = yield* Path.Path;
  const home = yield* HostProcess.HomeDirectory;
  const homePath = config.homePath.trim();
  if (homePath.length > 0) {
    return path.resolve(expandHomePath(homePath, home));
  }
  // Inherited env vars are not shell-expanded, so a literal `~` stays literal.
  const inherited = environment?.CLAUDE_CONFIG_DIR?.trim() ?? "";
  if (inherited.length > 0) {
    return path.resolve(inherited);
  }
  return path.resolve(path.join(home, ".claude"));
});

const decodeSettingsEnv = Schema.decodeUnknownEffect(
  fromLenientJson(
    Schema.Struct({ env: Schema.optional(Schema.Record(Schema.String, Schema.Unknown)) }),
  ),
);

/**
 * Claude Code copies each settings file's `env` over its own environment, so a user's
 * `OTEL_RESOURCE_ATTRIBUTES` there drops `t3code.launched`. Returns the flag-settings `env` that
 * restores it, or nothing when no file sets it, it already carries it, or the user chose a value.
 * `flagEnv` is the `env` of the user's own flag settings, which outrank every file.
 */
export const claudeSettingsLaunchedEnv = Effect.fn("claudeSettingsLaunchedEnv")(function* (
  cwd: string | null,
  environment: NodeJS.ProcessEnv,
  flagEnv?: Readonly<Record<string, unknown>>,
): Effect.fn.Return<
  Readonly<Record<string, string>> | undefined,
  never,
  FileSystem.FileSystem | Path.Path
> {
  const launched = userLaunchedAttribute(environment[RESOURCE_ATTRIBUTES] ?? "");
  if (launched !== undefined && launched !== "true") {
    return undefined;
  }
  const markLaunched = (attributes: string) =>
    userLaunchedAttribute(attributes) === undefined
      ? { [RESOURCE_ATTRIBUTES]: withLaunchedAttribute(attributes) }
      : undefined;
  const flagAttributes = flagEnv?.[RESOURCE_ATTRIBUTES];
  if (typeof flagAttributes === "string") {
    return markLaunched(flagAttributes);
  }
  const fileSystem = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const inheritedConfigDir = environment.CLAUDE_CONFIG_DIR?.trim() ?? "";
  const configDir =
    cwd !== null && inheritedConfigDir.length > 0 && !path.isAbsolute(inheritedConfigDir)
      ? path.resolve(cwd, inheritedConfigDir)
      : yield* resolveClaudeHomePath({ homePath: "" }, environment);
  const settingsPaths = [
    ...(cwd === null
      ? []
      : [
          path.join(cwd, ".claude", "settings.local.json"),
          path.join(cwd, ".claude", "settings.json"),
        ]),
    path.join(configDir, "settings.json"),
  ];
  for (const settingsPath of settingsPaths) {
    const contents = yield* fileSystem
      .readFileString(settingsPath)
      .pipe(Effect.orElseSucceed(() => undefined));
    if (contents === undefined) {
      continue;
    }
    const attributes = (yield* decodeSettingsEnv(contents).pipe(
      Effect.orElseSucceed(() => undefined),
    ))?.env?.[RESOURCE_ATTRIBUTES];
    if (typeof attributes !== "string") {
      continue;
    }
    return markLaunched(attributes);
  }
  return undefined;
});

export const makeClaudeEnvironment = Effect.fn("makeClaudeEnvironment")(function* (
  config: Pick<ClaudeSettings, "homePath">,
  baseEnv?: NodeJS.ProcessEnv,
): Effect.fn.Return<NodeJS.ProcessEnv, never, Path.Path> {
  const resolvedBaseEnv = baseEnv ?? process.env;
  const homePath = config.homePath.trim();
  if (homePath.length === 0) return resolvedBaseEnv;
  const resolvedHomePath = yield* resolveClaudeHomePath(config);
  return {
    ...resolvedBaseEnv,
    // Changing HOME hides macOS Keychain credentials; use CLAUDE_CONFIG_DIR to keep login access.
    CLAUDE_CONFIG_DIR: resolvedHomePath,
  };
});

/** managed-mcp.json excludes other MCP config and forbids --strict-mcp-config; omit the flag when present. */
export const hasClaudeManagedMcpConfig = Effect.fn("hasClaudeManagedMcpConfig")(function* () {
  const fileSystem = yield* FileSystem.FileSystem;
  const platform = yield* HostProcess.Platform;
  const managedMcpConfigPath =
    platform === "darwin"
      ? "/Library/Application Support/ClaudeCode/managed-mcp.json"
      : platform === "win32"
        ? "C:\\Program Files\\ClaudeCode\\managed-mcp.json"
        : "/etc/claude-code/managed-mcp.json";
  return yield* fileSystem.exists(managedMcpConfigPath).pipe(Effect.orElseSucceed(() => false));
});

export const makeClaudeContinuationGroupKey = Effect.fn("makeClaudeContinuationGroupKey")(
  function* (
    config: Pick<ClaudeSettings, "homePath">,
    environment?: NodeJS.ProcessEnv,
  ): Effect.fn.Return<string, never, Path.Path> {
    const resolvedHomePath = yield* resolveClaudeHomePath(config, environment);
    return `claude:home:${resolvedHomePath}`;
  },
);

export const makeClaudeCapabilitiesCacheKey = Effect.fn("makeClaudeCapabilitiesCacheKey")(
  function* (
    config: Pick<ClaudeSettings, "binaryPath" | "homePath">,
    cwd?: string,
    environment?: NodeJS.ProcessEnv,
  ): Effect.fn.Return<string, never, Path.Path> {
    const resolvedHomePath = yield* resolveClaudeHomePath(config, environment);
    return `${config.binaryPath}\0${resolvedHomePath}\0${cwd ?? ""}`;
  },
);

/** Separate spawn env from login commands to preserve literal paths, including relative inherited values. */
export const claudeSignedOutMessage = (input: {
  readonly configDir: string | undefined;
  readonly cwd: string;
}): string => {
  const configuration =
    input.configDir !== undefined
      ? ` from ${quotePath(input.cwd)}, with CLAUDE_CONFIG_DIR set to ${quotePath(input.configDir)}`
      : "";
  return `Claude could not authenticate. For subscription login, run \`claude auth login\` on this environment's machine${configuration}, then start a new thread. For API-key authentication, check this instance's configured credentials.`;
};
