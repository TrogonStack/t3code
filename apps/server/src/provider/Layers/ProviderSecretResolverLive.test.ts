import { describe, it, assert } from "@effect/vitest";
import { ProviderInstanceEnvironment } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Schema from "effect/Schema";
import * as Sink from "effect/Sink";
import * as Stream from "effect/Stream";
import { ChildProcessSpawner } from "effect/unstable/process";

import { ProviderSecretResolverLive } from "./ProviderSecretResolverLive.ts";
import { ProviderSecretResolver } from "../Services/ProviderSecretResolver.ts";

const encoder = new TextEncoder();
const decodeEnvironment = Schema.decodeSync(ProviderInstanceEnvironment);

const TOKEN_REFERENCE = "op://Private/claude-code/credential";

/**
 * Spawner that answers every `op read` with `result` and records the argv it
 * was handed, so tests can assert both the substituted value and how many
 * times 1Password was actually consulted.
 */
function recordingOpSpawner(result: { stdout: string; stderr: string; code: number }) {
  const invocations: Array<ReadonlyArray<string>> = [];
  const layer = Layer.succeed(
    ChildProcessSpawner.ChildProcessSpawner,
    ChildProcessSpawner.make((command) => {
      const cmd = command as unknown as { args: ReadonlyArray<string> };
      invocations.push(cmd.args);
      return Effect.succeed(
        ChildProcessSpawner.makeHandle({
          pid: ChildProcessSpawner.ProcessId(1),
          exitCode: Effect.succeed(ChildProcessSpawner.ExitCode(result.code)),
          isRunning: Effect.succeed(false),
          kill: () => Effect.void,
          unref: Effect.succeed(Effect.void),
          stdin: Sink.drain,
          stdout: Stream.make(encoder.encode(result.stdout)),
          stderr: Stream.make(encoder.encode(result.stderr)),
          all: Stream.empty,
          getInputFd: () => Sink.drain,
          getOutputFd: () => Stream.empty,
        }),
      );
    }),
  );
  return { layer, invocations };
}

describe("ProviderSecretResolverLive", () => {
  it.effect("leaves an environment of literal values alone", () => {
    const spawner = recordingOpSpawner({ stdout: "", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;
      const environment = decodeEnvironment([
        { name: "CLAUDE_SECURESTORAGE_CONFIG_DIR", value: "/home/u/.claude/work" },
      ]);

      const resolved = yield* resolver.resolve(environment);

      assert.deepStrictEqual(resolved, { variables: environment, unresolved: [] });
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(Effect.provide(ProviderSecretResolverLive.pipe(Layer.provide(spawner.layer))));
  });

  it.effect("swaps a secret reference for the value 1Password returns", () => {
    const spawner = recordingOpSpawner({ stdout: "sk-live-token\n", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          { name: "CLAUDE_SECURESTORAGE_CONFIG_DIR", value: "/home/u/.claude/work" },
          { name: "CLAUDE_CODE_OAUTH_TOKEN", value: TOKEN_REFERENCE, sensitive: true },
        ]),
      );

      assert.deepStrictEqual(
        resolved.variables?.map((variable) => [variable.name, variable.value]),
        [
          ["CLAUDE_SECURESTORAGE_CONFIG_DIR", "/home/u/.claude/work"],
          ["CLAUDE_CODE_OAUTH_TOKEN", "sk-live-token"],
        ],
      );
      assert.deepStrictEqual(spawner.invocations, [["read", "--no-newline", TOKEN_REFERENCE]]);
    }).pipe(Effect.provide(ProviderSecretResolverLive.pipe(Layer.provide(spawner.layer))));
  });

  it.effect("reads a reference once and holds it until the caller invalidates", () => {
    const spawner = recordingOpSpawner({ stdout: "sk-live-token", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;
      const environment = decodeEnvironment([
        { name: "CLAUDE_CODE_OAUTH_TOKEN", value: TOKEN_REFERENCE, sensitive: true },
      ]);

      // Every thread start and every instance rebuild resolves again; none of
      // them should reach 1Password while the value is already in memory.
      yield* resolver.resolve(environment);
      yield* resolver.resolve(environment);
      assert.strictEqual(spawner.invocations.length, 1);

      yield* resolver.invalidate;
      yield* resolver.resolve(environment);
      assert.strictEqual(spawner.invocations.length, 2);
    }).pipe(Effect.provide(ProviderSecretResolverLive.pipe(Layer.provide(spawner.layer))));
  });

  it.effect("drops a variable whose reference cannot be read", () => {
    const spawner = recordingOpSpawner({
      stdout: "",
      stderr: "[ERROR] could not read secret: not signed in",
      code: 1,
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      // An empty string here would look like a present credential and send
      // the provider off to fail mid-turn. Absence is the honest answer, and
      // the provider reports itself unauthenticated instead.
      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          { name: "CLAUDE_SECURESTORAGE_CONFIG_DIR", value: "/home/u/.claude/work" },
          { name: "CLAUDE_CODE_OAUTH_TOKEN", value: TOKEN_REFERENCE, sensitive: true },
        ]),
      );

      assert.deepStrictEqual(
        resolved.variables?.map((variable) => variable.name),
        ["CLAUDE_SECURESTORAGE_CONFIG_DIR"],
      );
      // Naming it is what gets it unset in the child environment. Merely
      // leaving it out would hand the provider whatever the server itself was
      // started with under that name.
      assert.deepStrictEqual(resolved.unresolved, ["CLAUDE_CODE_OAUTH_TOKEN"]);
    }).pipe(Effect.provide(ProviderSecretResolverLive.pipe(Layer.provide(spawner.layer))));
  });

  it.effect("holds a failed read too, so a locked vault prompts once", () => {
    const spawner = recordingOpSpawner({ stdout: "", stderr: "not signed in", code: 1 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;
      const environment = decodeEnvironment([
        { name: "CLAUDE_CODE_OAUTH_TOKEN", value: TOKEN_REFERENCE, sensitive: true },
      ]);

      yield* resolver.resolve(environment);
      yield* resolver.resolve(environment);

      assert.strictEqual(spawner.invocations.length, 1);
    }).pipe(Effect.provide(ProviderSecretResolverLive.pipe(Layer.provide(spawner.layer))));
  });
});
