import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";
import * as HostProcess from "@t3tools/shared/HostProcess";
import { expandHomePath } from "@t3tools/provider-core/server/pathExpansion";

// The SDK ignores ~/.cursor/cli-config.json and defaults commit/PR attribution to on.
const CursorCliConfig = Schema.Struct({
  attribution: Schema.optional(
    Schema.Struct({
      attributeCommitsToAgent: Schema.optional(Schema.Boolean),
      attributePRsToAgent: Schema.optional(Schema.Boolean),
    }),
  ),
});

const decodeCliConfig = Schema.decodeUnknownEffect(Schema.fromJsonString(CursorCliConfig));

const resolveHome = Effect.fn("CursorUserInstructions.resolveHome")(function* (
  environment: NodeJS.ProcessEnv,
) {
  return (
    environment.HOME?.trim() ||
    environment.USERPROFILE?.trim() ||
    (yield* HostProcess.HomeDirectory)
  );
});

const defaultCliConfigDirectory = Effect.fn("CursorUserInstructions.defaultCliConfigDirectory")(
  function* (environment: NodeJS.ProcessEnv, home: string) {
    const path = yield* Path.Path;
    const platform = yield* HostProcess.Platform;
    const configDir = environment.CURSOR_CONFIG_DIR?.trim();
    if (configDir) return expandHomePath(configDir, home);
    const xdgConfigHome = environment.XDG_CONFIG_HOME?.trim();
    if (platform !== "darwin" && platform !== "win32" && xdgConfigHome) {
      return path.join(xdgConfigHome, "cursor");
    }
    return path.join(home, ".cursor");
  },
);

export const cursorUserInstructions = Effect.fn("CursorUserInstructions.read")(function* (input: {
  readonly cliConfigPath: string | undefined;
  readonly instructionsPath: string | undefined;
  readonly environment: NodeJS.ProcessEnv;
}) {
  const fileSystem = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const home = yield* resolveHome(input.environment);
  const sections: Array<string> = [];

  if (input.instructionsPath !== undefined) {
    const instructionsPath = expandHomePath(input.instructionsPath, home);
    const contents = yield* fileSystem.readFileString(instructionsPath).pipe(
      Effect.map((text) => text.trim()),
      Effect.tapError((cause) =>
        Effect.logWarning("Cursor global instructions file is unreadable; skipping it.", {
          instructionsPath,
          cause,
        }),
      ),
      Effect.orElseSucceed(() => ""),
    );
    if (contents.length > 0) {
      sections.push(`## User instructions\n\n${contents}`);
    }
  }

  const cliConfigPath =
    input.cliConfigPath === undefined
      ? path.join(yield* defaultCliConfigDirectory(input.environment, home), "cli-config.json")
      : expandHomePath(input.cliConfigPath, home);
  const attribution = yield* fileSystem.readFileString(cliConfigPath).pipe(
    Effect.flatMap(decodeCliConfig),
    Effect.map((config) => config.attribution),
    Effect.orElseSucceed(() => undefined),
  );
  const targets = [
    attribution?.attributeCommitsToAgent === false ? "commit messages" : undefined,
    attribution?.attributePRsToAgent === false ? "pull request descriptions" : undefined,
  ].filter((target) => target !== undefined);
  if (targets.length > 0) {
    sections.push(
      `## Attribution\n\nDo not add Cursor attribution to ${targets.join(" or ")}: no "Made with Cursor" footers and no Co-authored-by Cursor trailers.`,
    );
  }

  return sections.length === 0 ? undefined : sections.join("\n\n");
});

export function prependCursorUserInstructions(input: {
  readonly prompt: string;
  readonly instructions: string | undefined;
  readonly runOrdinal: number;
}): string {
  return input.runOrdinal === 1 && input.instructions !== undefined
    ? `${input.instructions}\n\n${input.prompt}`
    : input.prompt;
}
