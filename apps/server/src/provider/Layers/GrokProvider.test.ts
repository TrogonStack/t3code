// @effect-diagnostics nodeBuiltinImport:off - locates the ACP mock agent for the fake Grok CLI.
import * as NodePath from "node:path";
import * as NodeURL from "node:url";

import * as NodeServices from "@effect/platform-node/NodeServices";
import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";
import { GrokSettings } from "@t3tools/contracts";

import { buildInitialGrokProviderSnapshot, checkGrokProviderStatus } from "./GrokProvider.ts";

const decodeGrokSettings = Schema.decodeSync(GrokSettings);

const mockAgentPath = NodePath.join(
  NodePath.dirname(NodeURL.fileURLToPath(import.meta.url)),
  "../../../scripts/acp-mock-agent.ts",
);

// The API-key branch is covered in GrokAcpSupport.test.ts; drop the key here so
// a developer's real credentials cannot change what the probe negotiates.
const { XAI_API_KEY: _ignoredApiKey, ...acpProbeEnv } = process.env;

/** A `grok` stand-in that answers `--version` itself and defers `agent stdio` to the ACP mock. */
const writeMockGrokCli = () =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const dir = yield* fs.makeTempDirectoryScoped({ prefix: "t3code-grok-acp-" });
    const grokPath = path.join(dir, "grok");
    yield* fs.writeFileString(
      grokPath,
      [
        "#!/bin/sh",
        'if [ "$1" = "--version" ]; then',
        '  printf "grok-cli 0.0.99\\n"',
        "  exit 0",
        "fi",
        // @effect-diagnostics-next-line preferSchemaOverJson:off - quotes paths for the shell wrapper.
        `exec ${JSON.stringify(process.execPath)} ${JSON.stringify(mockAgentPath)} "$@"`,
        "",
      ].join("\n"),
    );
    yield* fs.chmod(grokPath, 0o755);
    return grokPath;
  });

describe("buildInitialGrokProviderSnapshot", () => {
  it.effect("returns a disabled snapshot when settings.enabled is false", () =>
    Effect.gen(function* () {
      const snapshot = yield* buildInitialGrokProviderSnapshot(
        decodeGrokSettings({ enabled: false }),
      );
      expect(snapshot.enabled).toBe(false);
      expect(snapshot.status).toBe("disabled");
      expect(snapshot.installed).toBe(false);
      expect(snapshot.message).toContain("disabled");
    }),
  );

  it.effect("returns a pending snapshot by default", () =>
    Effect.gen(function* () {
      const snapshot = yield* buildInitialGrokProviderSnapshot(decodeGrokSettings({}));
      expect(snapshot.enabled).toBe(true);
      expect(snapshot.installed).toBe(true);
      expect(snapshot.status).toBe("warning");
      expect(snapshot.version).toBeNull();
      expect(snapshot.message).toContain("Checking Grok");
      expect(snapshot.requiresNewThreadForModelChange).toBe(true);
    }),
  );
});

it.layer(NodeServices.layer)("checkGrokProviderStatus", (it) => {
  it.effect("reports the binary as missing when the binary path does not resolve", () =>
    Effect.gen(function* () {
      const snapshot = yield* checkGrokProviderStatus(
        decodeGrokSettings({
          enabled: true,
          binaryPath: "/definitely/not/installed/grok-binary",
        }),
      );
      expect(snapshot.enabled).toBe(true);
      expect(snapshot.installed).toBe(false);
      expect(snapshot.status).toBe("error");
      expect(snapshot.message).toMatch(/not installed|not on PATH|Failed to execute/);
    }),
  );

  it.effect("reports an installed CLI as unhealthy when --version exits non-zero", () =>
    Effect.gen(function* () {
      const secretStderr = "broken grok install: secret-token-value";
      const snapshot = yield* Effect.scoped(
        Effect.gen(function* () {
          const fs = yield* FileSystem.FileSystem;
          const path = yield* Path.Path;
          const dir = yield* fs.makeTempDirectoryScoped({ prefix: "t3code-grok-version-" });
          const grokPath = path.join(dir, "grok");
          yield* fs.writeFileString(
            grokPath,
            ["#!/bin/sh", `printf "%s\\n" "${secretStderr}" >&2`, "exit 2", ""].join("\n"),
          );
          yield* fs.chmod(grokPath, 0o755);

          return yield* checkGrokProviderStatus(
            decodeGrokSettings({ enabled: true, binaryPath: grokPath }),
          );
        }),
      );

      expect(snapshot.enabled).toBe(true);
      expect(snapshot.installed).toBe(true);
      expect(snapshot.status).toBe("error");
      expect(snapshot.message).toBe("Grok CLI is installed but failed to run.");
      expect(snapshot.message).not.toContain(secretStderr);
    }),
  );

  it.effect("reports an error when ACP model discovery is unavailable", () =>
    Effect.gen(function* () {
      const snapshot = yield* Effect.scoped(
        Effect.gen(function* () {
          const fs = yield* FileSystem.FileSystem;
          const path = yield* Path.Path;
          const dir = yield* fs.makeTempDirectoryScoped({ prefix: "t3code-grok-success-" });
          const grokPath = path.join(dir, "grok");
          yield* fs.writeFileString(
            grokPath,
            ["#!/bin/sh", 'printf "grok-cli 0.0.99\\n"', "exit 0", ""].join("\n"),
          );
          yield* fs.chmod(grokPath, 0o755);

          return yield* checkGrokProviderStatus(
            decodeGrokSettings({ enabled: true, binaryPath: grokPath }),
          );
        }),
      );

      expect(snapshot.status).toBe("error");
      expect(snapshot.installed).toBe(true);
      expect(snapshot.models.map((model) => model.slug)).toEqual(["grok-build"]);
      expect(snapshot.message).toContain("ACP startup failed");
    }),
  );

  it.effect("reports the account the Grok CLI authenticates as", () =>
    Effect.gen(function* () {
      const snapshot = yield* Effect.scoped(
        Effect.gen(function* () {
          const grokPath = yield* writeMockGrokCli();
          return yield* checkGrokProviderStatus(
            decodeGrokSettings({ enabled: true, binaryPath: grokPath }),
            { ...acpProbeEnv, T3_ACP_AUTHENTICATE_EMAIL: "grok-user@example.com" },
          );
        }),
      );

      expect(snapshot.status).toBe("ready");
      expect(snapshot.installed).toBe(true);
      expect(snapshot.auth).toEqual({ status: "authenticated", email: "grok-user@example.com" });
    }),
  );

  it.effect("reports an unauthenticated CLI when the agent demands sign-in", () =>
    Effect.gen(function* () {
      const snapshot = yield* Effect.scoped(
        Effect.gen(function* () {
          const grokPath = yield* writeMockGrokCli();
          return yield* checkGrokProviderStatus(
            decodeGrokSettings({ enabled: true, binaryPath: grokPath }),
            { ...acpProbeEnv, T3_ACP_FAIL_AUTHENTICATE: "1" },
          );
        }),
      );

      expect(snapshot.status).toBe("error");
      expect(snapshot.installed).toBe(true);
      expect(snapshot.auth).toEqual({ status: "unauthenticated" });
      expect(snapshot.message).toContain("not authenticated");
    }),
  );
});
