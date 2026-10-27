import * as NodeOS from "node:os";

import * as NodeServices from "@effect/platform-node/NodeServices";
import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Path from "effect/Path";

import * as HostProcess from "@t3tools/shared/HostProcess";

import {
  claudeSettingsLaunchedEnv,
  makeClaudeCapabilitiesCacheKey,
  makeClaudeContinuationGroupKey,
  makeClaudeEnvironment,
  resolveClaudeHomePath,
} from "./ClaudeHome.ts";

it.layer(NodeServices.layer)("ClaudeHome", (it) => {
  describe("Claude home resolution", () => {
    it.effect("resolves configured Claude HOME and stamps continuation/cache keys with it", () =>
      Effect.gen(function* () {
        const path = yield* Path.Path;
        const homePath = "~/.claude-work";
        const resolved = path.resolve(NodeOS.homedir(), ".claude-work");

        expect(yield* resolveClaudeHomePath({ homePath })).toBe(resolved);
        expect((yield* makeClaudeEnvironment({ homePath })).CLAUDE_CONFIG_DIR).toBe(resolved);
        expect(yield* makeClaudeContinuationGroupKey({ homePath })).toBe(`claude:home:${resolved}`);
        expect(yield* makeClaudeCapabilitiesCacheKey({ binaryPath: "claude", homePath })).toBe(
          `claude\0${resolved}\0`,
        );
      }),
    );

    it.effect("uses inherited CLAUDE_CONFIG_DIR when homePath is empty", () =>
      Effect.gen(function* () {
        const path = yield* Path.Path;
        const inherited = path.resolve("/tmp/claude-inherited");
        const environment = { CLAUDE_CONFIG_DIR: inherited };

        expect(yield* resolveClaudeHomePath({ homePath: "" }, environment)).toBe(inherited);
        expect(yield* makeClaudeContinuationGroupKey({ homePath: "" }, environment)).toBe(
          `claude:home:${inherited}`,
        );

        const explicit = path.resolve(NodeOS.homedir(), ".claude-work");
        expect(yield* resolveClaudeHomePath({ homePath: "~/.claude-work" }, environment)).toBe(
          explicit,
        );
      }),
    );

    it.effect("separates capability probes by cwd", () =>
      Effect.gen(function* () {
        const config = { binaryPath: "claude", homePath: "" };
        const first = yield* makeClaudeCapabilitiesCacheKey(config, "/repo-a");
        const second = yield* makeClaudeCapabilitiesCacheKey(config, "/repo-b");
        expect(first).not.toBe(second);
      }),
    );
  });

  describe("settings launched env", () => {
    const LAUNCHED = "t3code.launched=true";
    const setup = (files: {
      readonly user?: string;
      readonly project?: string;
      readonly local?: string;
    }) =>
      Effect.gen(function* () {
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const root = yield* fileSystem.makeTempDirectoryScoped();
        const configDir = path.join(root, "config");
        const cwd = path.join(root, "workspace");
        yield* fileSystem.makeDirectory(configDir, { recursive: true });
        yield* fileSystem.makeDirectory(path.join(cwd, ".claude"), { recursive: true });
        if (files.user !== undefined) {
          yield* fileSystem.writeFileString(path.join(configDir, "settings.json"), files.user);
        }
        if (files.project !== undefined) {
          yield* fileSystem.writeFileString(
            path.join(cwd, ".claude", "settings.json"),
            files.project,
          );
        }
        if (files.local !== undefined) {
          yield* fileSystem.writeFileString(
            path.join(cwd, ".claude", "settings.local.json"),
            files.local,
          );
        }
        return { cwd, environment: { CLAUDE_CONFIG_DIR: configDir } };
      });
    const settingsWith = (attributes: string) =>
      JSON.stringify({ env: { OTEL_RESOURCE_ATTRIBUTES: attributes } });

    it.effect("extends the value of the highest precedence settings file", () =>
      Effect.gen(function* () {
        const { cwd, environment } = yield* setup({
          user: settingsWith("from=user"),
          project: settingsWith("from=project"),
          local: settingsWith("from=local"),
        });
        expect(yield* claudeSettingsLaunchedEnv(cwd, environment)).toEqual({
          OTEL_RESOURCE_ATTRIBUTES: `from=local,${LAUNCHED}`,
        });
        const withoutLocal = yield* setup({
          user: settingsWith("from=user"),
          project: settingsWith("from=project"),
        });
        expect(
          yield* claudeSettingsLaunchedEnv(withoutLocal.cwd, withoutLocal.environment),
        ).toEqual({ OTEL_RESOURCE_ATTRIBUTES: `from=project,${LAUNCHED}` });
      }).pipe(Effect.scoped, Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
    );

    it.effect("extends the --settings launch argument value over every settings file", () =>
      Effect.gen(function* () {
        const { cwd, environment } = yield* setup({ local: settingsWith("from=local") });
        expect(
          yield* claudeSettingsLaunchedEnv(cwd, environment, {
            OTEL_RESOURCE_ATTRIBUTES: "from=flag",
          }),
        ).toEqual({ OTEL_RESOURCE_ATTRIBUTES: `from=flag,${LAUNCHED}` });
      }).pipe(Effect.scoped, Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
    );

    it.effect("reads the user file when there is no workspace", () =>
      Effect.gen(function* () {
        const { environment } = yield* setup({ user: settingsWith("org.name=Acme") });
        expect(yield* claudeSettingsLaunchedEnv(null, environment)).toEqual({
          OTEL_RESOURCE_ATTRIBUTES: `org.name=Acme,${LAUNCHED}`,
        });
      }).pipe(Effect.scoped, Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
    );

    it.effect("changes nothing when no settings file overrides the variable", () =>
      Effect.gen(function* () {
        const none = yield* setup({});
        expect(yield* claudeSettingsLaunchedEnv(none.cwd, none.environment)).toBeUndefined();
        const unrelated = yield* setup({ user: JSON.stringify({ env: { OTHER: "1" } }) });
        expect(
          yield* claudeSettingsLaunchedEnv(unrelated.cwd, unrelated.environment),
        ).toBeUndefined();
      }).pipe(Effect.scoped, Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
    );

    it.effect("resolves a relative config dir against the workspace", () =>
      Effect.gen(function* () {
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const { cwd } = yield* setup({});
        yield* fileSystem.makeDirectory(path.join(cwd, "claude-config"), { recursive: true });
        yield* fileSystem.writeFileString(
          path.join(cwd, "claude-config", "settings.json"),
          settingsWith("org.name=Acme"),
        );
        expect(
          yield* claudeSettingsLaunchedEnv(cwd, { CLAUDE_CONFIG_DIR: "claude-config" }),
        ).toEqual({ OTEL_RESOURCE_ATTRIBUTES: `org.name=Acme,${LAUNCHED}` });
      }).pipe(Effect.scoped, Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
    );

    it.effect("skips an unreadable file and keeps looking", () =>
      Effect.gen(function* () {
        const { cwd, environment } = yield* setup({
          user: settingsWith("from=user"),
          local: "{ not json",
        });
        expect(yield* claudeSettingsLaunchedEnv(cwd, environment)).toEqual({
          OTEL_RESOURCE_ATTRIBUTES: `from=user,${LAUNCHED}`,
        });
      }).pipe(Effect.scoped, Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
    );

    it.effect("respects a launched value the user chose", () =>
      Effect.gen(function* () {
        const inFile = yield* setup({ user: settingsWith("t3code.launched=false") });
        expect(yield* claudeSettingsLaunchedEnv(inFile.cwd, inFile.environment)).toBeUndefined();
        const optedOut = yield* setup({ user: settingsWith("from=user") });
        expect(
          yield* claudeSettingsLaunchedEnv(optedOut.cwd, {
            ...optedOut.environment,
            OTEL_RESOURCE_ATTRIBUTES: "t3code.launched=false",
          }),
        ).toBeUndefined();
      }).pipe(Effect.scoped, Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
    );
  });
});
