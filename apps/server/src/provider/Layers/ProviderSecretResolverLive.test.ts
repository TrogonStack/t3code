// @effect-diagnostics nodeBuiltinImport:off - asserts against the fake op CLI script this test writes to disk.
import { describe, it, assert } from "@effect/vitest";
import { ProviderInstanceEnvironment } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Schema from "effect/Schema";
import * as Sink from "effect/Sink";
import * as Stream from "effect/Stream";
import { ChildProcessSpawner } from "effect/unstable/process";
import * as NodeFileSystem from "@effect/platform-node/NodeFileSystem";
import * as NodeFS from "node:fs";

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
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
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
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
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
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
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
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
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
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
  });
});

const SECOND_REFERENCE = "op://Private/codex/credential";

/**
 * Spawner that answers each `op` invocation from `handler`, which is handed the
 * argv and, for `op inject`, the template `op` would have read.
 *
 * The template is read back off disk through the `-i` path in the argv, which
 * is how `op` itself receives it. Reading it any other way would let a change
 * that stops writing the file pass, and that is the shape of the bug this
 * spawner exists to catch.
 *
 * The template matters: `prime` picks a random separator per call, so a test
 * cannot hard-code the output. Recovering the separator from the template is
 * also what proves the batch and the split agree on a format.
 */
function scriptedOpSpawner(
  handler: (
    args: ReadonlyArray<string>,
    template: string,
  ) => { stdout: string; stderr: string; code: number },
) {
  const invocations: Array<ReadonlyArray<string>> = [];
  const stdinUses: Array<boolean> = [];
  const layer = Layer.succeed(
    ChildProcessSpawner.ChildProcessSpawner,
    ChildProcessSpawner.make((command) =>
      Effect.sync(() => {
        const cmd = command as unknown as {
          args: ReadonlyArray<string>;
          options?: { stdin?: Stream.Stream<Uint8Array> };
        };
        invocations.push(cmd.args);
        stdinUses.push(cmd.options?.stdin !== undefined);
        const inputPath = cmd.args[cmd.args.indexOf("-i") + 1];
        const template =
          cmd.args.includes("-i") && inputPath !== undefined
            ? NodeFS.readFileSync(inputPath, "utf8")
            : "";
        const result = handler(cmd.args, template);
        return ChildProcessSpawner.makeHandle({
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
        });
      }),
    ),
  );
  return { layer, invocations, stdinUses };
}

/** The separator `prime` chose, read back out of the template it built. */
function separatorOf(template: string): string {
  const parts = template.split(/\{\{[^}]*\}\}/);
  return parts[1] ?? "";
}

describe("ProviderSecretResolverLive.prime", () => {
  it.effect("reads every reference in a single 1Password call", () => {
    const spawner = scriptedOpSpawner((args, template) => {
      if (args.includes("inject")) {
        return {
          stdout: ["sk-claude-token", "sk-codex-token"].join(separatorOf(template)),
          stderr: "",
          code: 0,
        };
      }
      return { stdout: "should-not-be-read-one-at-a-time", stderr: "", code: 0 };
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.prime([TOKEN_REFERENCE, SECOND_REFERENCE]);

      assert.strictEqual(spawner.invocations.length, 1);
      assert.deepStrictEqual(Array.from(spawner.invocations[0] ?? []).slice(0, 2), [
        "inject",
        "-i",
      ]);

      // Both instances resolve out of the primed cache, so the fleet costs the
      // one authorization the batch already paid for.
      const claude = yield* resolver.resolve(
        decodeEnvironment([
          { name: "CLAUDE_CODE_OAUTH_TOKEN", value: TOKEN_REFERENCE, sensitive: true },
        ]),
      );
      const codex = yield* resolver.resolve(
        decodeEnvironment([{ name: "CODEX_TOKEN", value: SECOND_REFERENCE, sensitive: true }]),
      );

      assert.strictEqual(claude.variables?.[0]?.value, "sk-claude-token");
      assert.strictEqual(codex.variables?.[0]?.value, "sk-codex-token");
      assert.strictEqual(spawner.invocations.length, 1);
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
  });

  it.effect("falls back to one read at a time when the batch fails", () => {
    const spawner = scriptedOpSpawner((args) => {
      if (args.includes("inject")) {
        return { stdout: "", stderr: 'could not resolve item "codex"', code: 1 };
      }
      return args.includes(SECOND_REFERENCE)
        ? { stdout: "", stderr: 'could not resolve item "codex"', code: 1 }
        : { stdout: "sk-claude-token", stderr: "", code: 0 };
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.prime([TOKEN_REFERENCE, SECOND_REFERENCE]);

      // The batch is still attempted; it is the recovery that is per reference.
      assert.strictEqual(spawner.invocations[0]?.[0], "inject");

      // A batch that cannot be trusted leaves the cache cold rather than
      // caching a failure for every reference in it, so the good reference
      // still resolves and only the bad one is reported unresolved.
      const claude = yield* resolver.resolve(
        decodeEnvironment([
          { name: "CLAUDE_CODE_OAUTH_TOKEN", value: TOKEN_REFERENCE, sensitive: true },
        ]),
      );
      const codex = yield* resolver.resolve(
        decodeEnvironment([{ name: "CODEX_TOKEN", value: SECOND_REFERENCE, sensitive: true }]),
      );

      assert.strictEqual(claude.variables?.[0]?.value, "sk-claude-token");
      assert.deepStrictEqual(Array.from(claude.unresolved), []);
      assert.deepStrictEqual(Array.from(codex.unresolved), ["CODEX_TOKEN"]);
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
  });

  it.effect("hands the template to a file `op` will actually read", () => {
    let seenTemplate = "";
    const spawner = scriptedOpSpawner((args, template) => {
      if (args.includes("inject")) {
        seenTemplate = template;
        return {
          stdout: ["sk-claude-token", "sk-codex-token"].join(separatorOf(template)),
          stderr: "",
          code: 0,
        };
      }
      return { stdout: "should-not-be-read-one-at-a-time", stderr: "", code: 0 };
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.prime([TOKEN_REFERENCE, SECOND_REFERENCE]);

      // `op` only reads piped input from a named pipe, and Node hands a child
      // a socket pair, so a template offered on stdin is never seen and the
      // batch fails every time. The `-i` path is the delivery that works.
      const args = Array.from(spawner.invocations[0] ?? []);
      assert.deepStrictEqual(args.slice(0, 2), ["inject", "-i"]);
      assert.isTrue((args[2] ?? "").length > 0);
      assert.deepStrictEqual(spawner.stdinUses, [false]);

      // The file `op` was pointed at held both references and nothing else,
      // so a secret never reaches the disk.
      assert.isTrue(seenTemplate.includes(TOKEN_REFERENCE));
      assert.isTrue(seenTemplate.includes(SECOND_REFERENCE));
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
  });

  it.effect("leaves no template behind once the batch is done", () => {
    let templatePath = "";
    const spawner = scriptedOpSpawner((args, template) => {
      if (args.includes("inject")) {
        templatePath = args[args.indexOf("-i") + 1] ?? "";
        return {
          stdout: ["sk-claude-token", "sk-codex-token"].join(separatorOf(template)),
          stderr: "",
          code: 0,
        };
      }
      return { stdout: "", stderr: "", code: 0 };
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.prime([TOKEN_REFERENCE, SECOND_REFERENCE]);

      assert.isTrue(templatePath.length > 0);
      assert.isFalse(NodeFS.existsSync(templatePath));
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
  });

  it.effect("does not spawn a batch for a single reference", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "sk-claude-token", stderr: "", code: 0 }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.prime([TOKEN_REFERENCE]);

      // One reference is one prompt either way, and `op read` names the
      // reference it could not resolve.
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        ProviderSecretResolverLive.pipe(
          Layer.provide(Layer.merge(spawner.layer, NodeFileSystem.layer)),
        ),
      ),
    );
  });
});
