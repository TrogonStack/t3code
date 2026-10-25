// @effect-diagnostics nodeBuiltinImport:off - asserts against the fake op CLI script this test writes to disk.
import { describe, it, assert } from "@effect/vitest";
import {
  OnePasswordAccount,
  ProviderInstanceEnvironment,
  SecretProviderId,
  type ServerSettings,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as HostProcess from "@t3tools/shared/HostProcess";
import { HttpClient, HttpClientResponse } from "effect/http";
import * as NodePath from "@effect/platform-node/NodePath";
import * as Layer from "effect/Layer";
import * as Logger from "effect/Logger";
import * as Schema from "effect/Schema";
import * as Sink from "effect/Sink";
import * as Stream from "effect/Stream";
import * as TestClock from "effect/testing/TestClock";
import { ChildProcessSpawner } from "effect/process";
import * as NodeFileSystem from "@effect/platform-node/NodeFileSystem";
import * as NodeFS from "node:fs";

import * as Settings from "../serverSettings.ts";
import {
  NamedProviderSecretReference,
  OnePasswordSecretReference,
} from "./ProviderSecretReference.ts";
import { ProviderSecretResolverLive } from "./ProviderSecretResolverLive.ts";
import { ProviderSecretResolver } from "./ProviderSecretResolver.ts";

const encoder = new TextEncoder();
const decodeEnvironment = Schema.decodeSync(ProviderInstanceEnvironment);

const TOKEN_REFERENCE = "op://Private/claude-code/credential";

const HOME_ACCOUNT = "my.1password.com";
const WORK_ACCOUNT = "acme.1password.com";

const onePasswordVariable = (name: string, reference: string, account: string) => ({
  name,
  value: { kind: "1password" as const, reference, account },
});

const onePassword = (reference: string, account: string) => {
  const [variable] = decodeEnvironment([onePasswordVariable("TOKEN", reference, account)]);
  const source = variable?.value;
  if (source === undefined || typeof source === "string" || source.kind !== "1password") {
    throw new Error("expected a 1Password source");
  }
  return new OnePasswordSecretReference({ reference: source.reference, account: source.account });
};

/**
 * The live resolver over a fake spawner. `secretProviders` seeds the settings
 * the resolver looks named providers up in.
 */
const noHttpClient = HttpClient.make(() => Effect.die("unexpected HTTP request"));

const resolverLayer = (
  spawner: Layer.Layer<ChildProcessSpawner.ChildProcessSpawner>,
  secretProviders: ServerSettings["secretProviders"] = {},
  httpClient: HttpClient.HttpClient = noHttpClient,
) =>
  ProviderSecretResolverLive.pipe(
    Layer.provide(
      Layer.mergeAll(
        spawner,
        NodeFileSystem.layer,
        NodePath.layer,
        Layer.succeed(HttpClient.HttpClient, httpClient),
        Settings.layerTest({ secretProviders }),
      ),
    ),
  );

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

      assert.deepStrictEqual(resolved, {
        variables: [
          {
            name: "CLAUDE_SECURESTORAGE_CONFIG_DIR",
            value: "/home/u/.claude/work",
            sensitive: false,
          },
        ],
        unresolved: [],
      });
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("swaps a secret reference for the value 1Password returns", () => {
    const spawner = recordingOpSpawner({ stdout: "sk-live-token\n", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          { name: "CLAUDE_SECURESTORAGE_CONFIG_DIR", value: "/home/u/.claude/work" },
          onePasswordVariable("CLAUDE_CODE_OAUTH_TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT),
        ]),
      );

      assert.deepStrictEqual(
        resolved.variables?.map((variable) => [variable.name, variable.value]),
        [
          ["CLAUDE_SECURESTORAGE_CONFIG_DIR", "/home/u/.claude/work"],
          ["CLAUDE_CODE_OAUTH_TOKEN", "sk-live-token"],
        ],
      );
      assert.deepStrictEqual(spawner.invocations, [
        ["--cache=false", "read", "--account", HOME_ACCOUNT, "--no-newline", TOKEN_REFERENCE],
      ]);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("reads a reference once and holds it until the caller invalidates", () => {
    const spawner = recordingOpSpawner({ stdout: "sk-live-token", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;
      const environment = decodeEnvironment([
        onePasswordVariable("CLAUDE_CODE_OAUTH_TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT),
      ]);

      // Every thread start and every instance rebuild resolves again; none of
      // them should reach 1Password while the value is already in memory.
      yield* resolver.resolve(environment);
      yield* resolver.resolve(environment);
      assert.strictEqual(spawner.invocations.length, 1);

      yield* resolver.invalidate();
      yield* resolver.resolve(environment);
      assert.strictEqual(spawner.invocations.length, 2);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
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
          onePasswordVariable("CLAUDE_CODE_OAUTH_TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT),
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
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("reads a failed reference again on the next resolve", () => {
    const spawner = recordingOpSpawner({ stdout: "", stderr: "not signed in", code: 1 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;
      const environment = decodeEnvironment([
        onePasswordVariable("CLAUDE_CODE_OAUTH_TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT),
      ]);

      yield* resolver.resolve(environment);
      yield* resolver.resolve(environment);

      assert.strictEqual(spawner.invocations.length, 2);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });
});

const SECOND_REFERENCE = "op://Private/codex/credential";

/**
 * Spawner that answers each `op` invocation from `handler`. The `op inject` template is read
 * back from the `-i` path in argv, and its random separator proves batch and split agree.
 */
function scriptedOpSpawner(
  handler: (
    args: ReadonlyArray<string>,
    template: string,
  ) => { stdout: string; stderr: string; code: number },
) {
  const invocations: Array<ReadonlyArray<string>> = [];
  const commands: Array<string> = [];
  const environments: Array<Readonly<Record<string, string | undefined>> | undefined> = [];
  const stdinUses: Array<boolean> = [];
  const layer = Layer.succeed(
    ChildProcessSpawner.ChildProcessSpawner,
    ChildProcessSpawner.make((command) =>
      Effect.sync(() => {
        const cmd = command as unknown as {
          command: string;
          args: ReadonlyArray<string>;
          options?: {
            stdin?: Stream.Stream<Uint8Array>;
            env?: Readonly<Record<string, string | undefined>>;
          };
        };
        invocations.push(cmd.args);
        commands.push(cmd.command);
        environments.push(cmd.options?.env);
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
  return { layer, invocations, commands, environments, stdinUses };
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

      yield* resolver.prime([
        onePassword(TOKEN_REFERENCE, HOME_ACCOUNT),
        onePassword(SECOND_REFERENCE, HOME_ACCOUNT),
      ]);

      assert.strictEqual(spawner.invocations.length, 1);
      assert.deepStrictEqual(Array.from(spawner.invocations[0] ?? []).slice(0, 5), [
        "--cache=false",
        "inject",
        "--account",
        HOME_ACCOUNT,
        "-i",
      ]);

      // Both instances resolve out of the primed cache, so the fleet costs the
      // one authorization the batch already paid for.
      const claude = yield* resolver.resolve(
        decodeEnvironment([
          onePasswordVariable("CLAUDE_CODE_OAUTH_TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT),
        ]),
      );
      const codex = yield* resolver.resolve(
        decodeEnvironment([onePasswordVariable("CODEX_TOKEN", SECOND_REFERENCE, HOME_ACCOUNT)]),
      );

      assert.strictEqual(claude.variables?.[0]?.value, "sk-claude-token");
      assert.strictEqual(codex.variables?.[0]?.value, "sk-codex-token");
      assert.strictEqual(spawner.invocations.length, 1);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
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

      yield* resolver.prime([
        onePassword(TOKEN_REFERENCE, HOME_ACCOUNT),
        onePassword(SECOND_REFERENCE, HOME_ACCOUNT),
      ]);

      // The batch is still attempted; it is the recovery that is per reference.
      assert.strictEqual(spawner.invocations[0]?.[1], "inject");

      // A batch that cannot be trusted leaves the cache cold rather than
      // caching a failure for every reference in it, so the good reference
      // still resolves and only the bad one is reported unresolved.
      const claude = yield* resolver.resolve(
        decodeEnvironment([
          onePasswordVariable("CLAUDE_CODE_OAUTH_TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT),
        ]),
      );
      const codex = yield* resolver.resolve(
        decodeEnvironment([onePasswordVariable("CODEX_TOKEN", SECOND_REFERENCE, HOME_ACCOUNT)]),
      );

      assert.strictEqual(claude.variables?.[0]?.value, "sk-claude-token");
      assert.deepStrictEqual(Array.from(claude.unresolved), []);
      assert.deepStrictEqual(Array.from(codex.unresolved), ["CODEX_TOKEN"]);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
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

      yield* resolver.prime([
        onePassword(TOKEN_REFERENCE, HOME_ACCOUNT),
        onePassword(SECOND_REFERENCE, HOME_ACCOUNT),
      ]);

      // `op` only reads piped input from a named pipe, and Node hands a child
      // a socket pair, so a template offered on stdin is never seen and the
      // batch fails every time. The `-i` path is the delivery that works.
      const args = Array.from(spawner.invocations[0] ?? []);
      assert.deepStrictEqual(args.slice(0, 5), [
        "--cache=false",
        "inject",
        "--account",
        HOME_ACCOUNT,
        "-i",
      ]);
      assert.isTrue((args[5] ?? "").length > 0);
      assert.deepStrictEqual(spawner.stdinUses, [false]);

      // The file `op` was pointed at held both references and nothing else,
      // so a secret never reaches the disk.
      assert.isTrue(seenTemplate.includes(TOKEN_REFERENCE));
      assert.isTrue(seenTemplate.includes(SECOND_REFERENCE));
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
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

      yield* resolver.prime([
        onePassword(TOKEN_REFERENCE, HOME_ACCOUNT),
        onePassword(SECOND_REFERENCE, HOME_ACCOUNT),
      ]);

      assert.isTrue(templatePath.length > 0);
      assert.isFalse(NodeFS.existsSync(templatePath));
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("does not spawn a batch for a single reference", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "sk-claude-token", stderr: "", code: 0 }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.prime([onePassword(TOKEN_REFERENCE, HOME_ACCOUNT)]);

      // One reference is one prompt either way, and `op read` names the
      // reference it could not resolve.
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });
});

describe("ProviderSecretResolverLive with 1Password accounts", () => {
  it.effect("reads a 1Password source from the account it names", () => {
    const spawner = recordingOpSpawner({ stdout: "sk-work-token", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([onePasswordVariable("CODEX_TOKEN", TOKEN_REFERENCE, WORK_ACCOUNT)]),
      );

      assert.deepStrictEqual(resolved, {
        variables: [{ name: "CODEX_TOKEN", value: "sk-work-token", sensitive: true }],
        unresolved: [],
      });
      assert.deepStrictEqual(spawner.invocations, [
        ["--cache=false", "read", "--account", WORK_ACCOUNT, "--no-newline", TOKEN_REFERENCE],
      ]);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("keeps the same reference in two accounts apart", () => {
    const spawner = scriptedOpSpawner((args) => ({
      stdout: args.includes(WORK_ACCOUNT) ? "sk-work-token" : "sk-home-token",
      stderr: "",
      code: 0,
    }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          onePasswordVariable("HOME_TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT),
          onePasswordVariable("WORK_TOKEN", TOKEN_REFERENCE, WORK_ACCOUNT),
        ]),
      );

      assert.deepStrictEqual(resolved.variables, [
        { name: "HOME_TOKEN", value: "sk-home-token", sensitive: true },
        { name: "WORK_TOKEN", value: "sk-work-token", sensitive: true },
      ]);
      assert.strictEqual(spawner.invocations.length, 2);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("primes one batch per account", () => {
    const spawner = scriptedOpSpawner((args, template) => {
      if (!args.includes("inject")) {
        return { stdout: "should-not-be-read-one-at-a-time", stderr: "", code: 0 };
      }
      const prefix = args.includes(WORK_ACCOUNT) ? "work" : "home";
      return {
        stdout: [`${prefix}-claude`, `${prefix}-codex`].join(separatorOf(template)),
        stderr: "",
        code: 0,
      };
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.prime([
        onePassword(TOKEN_REFERENCE, HOME_ACCOUNT),
        onePassword(TOKEN_REFERENCE, WORK_ACCOUNT),
        onePassword(SECOND_REFERENCE, HOME_ACCOUNT),
        onePassword(SECOND_REFERENCE, WORK_ACCOUNT),
      ]);

      assert.deepStrictEqual(
        spawner.invocations.map((args) => Array.from(args).slice(0, 5)),
        [
          ["--cache=false", "inject", "--account", HOME_ACCOUNT, "-i"],
          ["--cache=false", "inject", "--account", WORK_ACCOUNT, "-i"],
        ],
      );

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          onePasswordVariable("HOME_CODEX", SECOND_REFERENCE, HOME_ACCOUNT),
          onePasswordVariable("WORK_CLAUDE", TOKEN_REFERENCE, WORK_ACCOUNT),
        ]),
      );
      assert.deepStrictEqual(resolved.variables, [
        { name: "HOME_CODEX", value: "home-codex", sensitive: true },
        { name: "WORK_CLAUDE", value: "work-claude", sensitive: true },
      ]);
      assert.strictEqual(spawner.invocations.length, 2);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });
});

describe("ProviderSecretResolverLive.listOnePasswordAccounts", () => {
  const provideResolver = (spawner: ReturnType<typeof recordingOpSpawner>) =>
    Effect.provide(resolverLayer(spawner.layer));

  it.effect("offers each signed-in account by the address `op --account` accepts", () => {
    const spawner = recordingOpSpawner({
      stdout: JSON.stringify([
        { url: HOME_ACCOUNT, email: "me@example.com", user_uuid: "U1", account_uuid: "A1" },
        { url: WORK_ACCOUNT, email: "me@acme.example", user_uuid: "U2", account_uuid: "A2" },
      ]),
      stderr: "",
      code: 0,
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const accounts = yield* resolver.listOnePasswordAccounts;

      assert.deepStrictEqual(accounts, [
        { account: OnePasswordAccount.make(HOME_ACCOUNT), email: "me@example.com" },
        { account: OnePasswordAccount.make(WORK_ACCOUNT), email: "me@acme.example" },
      ]);
      assert.deepStrictEqual(spawner.invocations, [
        ["--cache=false", "account", "list", "--format", "json"],
      ]);
    }).pipe(provideResolver(spawner));
  });

  it.effect("offers no accounts when `op` fails", () => {
    const spawner = recordingOpSpawner({ stdout: "", stderr: "not configured", code: 1 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;
      assert.deepStrictEqual(yield* resolver.listOnePasswordAccounts, []);
    }).pipe(provideResolver(spawner));
  });

  it.effect("offers no accounts when `op` prints something unexpected", () => {
    const spawner = recordingOpSpawner({ stdout: "not json", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;
      assert.deepStrictEqual(yield* resolver.listOnePasswordAccounts, []);
    }).pipe(provideResolver(spawner));
  });
});

const HOME_PROVIDER = SecretProviderId.make("home-1password");
const VAULT_PROVIDER = SecretProviderId.make("team-vault");
const BAO_ADDRESS = "https://bao.example.com:8200";

const namedVariable = (name: string, provider: SecretProviderId, reference: string) => ({
  name,
  value: { kind: "secret-provider" as const, provider, reference },
});

const named = (provider: SecretProviderId, reference: string) =>
  new NamedProviderSecretReference({ provider, reference });

const onePasswordProvider = {
  kind: "1password" as const,
  label: "Home",
  account: OnePasswordAccount.make(HOME_ACCOUNT),
};

const openBaoProvider = (
  overrides: {
    namespace?: string;
    token?: string;
    auth?: "token" | "env" | "cli";
    address?: string;
  } = {},
) => ({
  kind: "openbao" as const,
  label: "Team vault",
  address: BAO_ADDRESS,
  mount: "kv",
  auth: "token" as const,
  ...overrides,
});

interface RecordedRequest {
  readonly url: string;
  readonly headers: Readonly<Record<string, string>>;
}

/** HTTP client that answers every request with `status` and `body`, recording what it was sent. */
const recordingHttpClient = (status: number, body: unknown) => {
  const requests: Array<RecordedRequest> = [];
  const client = HttpClient.make((request) => {
    requests.push({ url: request.url, headers: { ...request.headers } });
    return Effect.succeed(
      HttpClientResponse.fromWeb(
        request,
        new Response(JSON.stringify(body), {
          status,
          headers: { "content-type": "application/json" },
        }),
      ),
    );
  });
  return { client, requests };
};

const hostLayer = (env: NodeJS.ProcessEnv) => Layer.succeed(HostProcess.Environment, env);

const apiRead = (reference = "agents/codex#api_key") =>
  decodeEnvironment([namedVariable("CODEX_TOKEN", VAULT_PROVIDER, reference)]);

describe("ProviderSecretResolverLive with named secret providers", () => {
  it.effect("reads a named 1Password provider from the account it is configured with", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "sk-home-token", stderr: "", code: 0 }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([namedVariable("CODEX_TOKEN", HOME_PROVIDER, TOKEN_REFERENCE)]),
      );

      assert.deepStrictEqual(resolved, {
        variables: [{ name: "CODEX_TOKEN", value: "sk-home-token", sensitive: true }],
        unresolved: [],
      });
      assert.deepStrictEqual(spawner.invocations, [
        ["--cache=false", "read", "--account", HOME_ACCOUNT, "--no-newline", TOKEN_REFERENCE],
      ]);
    }).pipe(Effect.provide(resolverLayer(spawner.layer, { [HOME_PROVIDER]: onePasswordProvider })));
  });

  it.effect("reports an OpenBao reference without a field unresolved without a request", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "whole-secret", stderr: "", code: 0 }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          namedVariable("NO_FIELD", VAULT_PROVIDER, "agents/codex"),
          namedVariable("NO_PATH", VAULT_PROVIDER, "#api_key"),
          namedVariable("FLAG_PATH", VAULT_PROVIDER, "-address=evil#api_key"),
        ]),
      );

      assert.deepStrictEqual(Array.from(resolved.unresolved), ["NO_FIELD", "NO_PATH", "FLAG_PATH"]);
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: openBaoProvider({ token: "s.t" }) },
          noHttpClient,
        ),
      ),
    );
  });

  it.effect("reports a reference to a provider that is not configured unresolved", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "sk-token", stderr: "", code: 0 }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([namedVariable("CODEX_TOKEN", VAULT_PROVIDER, "agents/codex#api_key")]),
      );

      assert.deepStrictEqual(resolved, { variables: [], unresolved: ["CODEX_TOKEN"] });
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("batches named 1Password references with legacy ones from the same account", () => {
    const spawner = scriptedOpSpawner((args, template) => {
      if (args.includes("inject")) {
        return {
          stdout: ["sk-shared", "sk-codex"].join(separatorOf(template)),
          stderr: "",
          code: 0,
        };
      }
      return { stdout: "should-not-be-read-one-at-a-time", stderr: "", code: 0 };
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.prime([
        onePassword(TOKEN_REFERENCE, HOME_ACCOUNT),
        named(HOME_PROVIDER, TOKEN_REFERENCE),
        named(HOME_PROVIDER, SECOND_REFERENCE),
        named(VAULT_PROVIDER, "agents/codex#api_key"),
      ]);

      // OpenBao has no unlock prompt to save, so priming leaves it alone and
      // the shared item is one slot in the template.
      assert.strictEqual(spawner.invocations.length, 1);
      assert.deepStrictEqual(Array.from(spawner.invocations[0] ?? []).slice(0, 5), [
        "--cache=false",
        "inject",
        "--account",
        HOME_ACCOUNT,
        "-i",
      ]);

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          onePasswordVariable("LEGACY", TOKEN_REFERENCE, HOME_ACCOUNT),
          namedVariable("NAMED", HOME_PROVIDER, TOKEN_REFERENCE),
          namedVariable("CODEX", HOME_PROVIDER, SECOND_REFERENCE),
        ]),
      );
      assert.deepStrictEqual(resolved.variables, [
        { name: "LEGACY", value: "sk-shared", sensitive: true },
        { name: "NAMED", value: "sk-shared", sensitive: true },
        { name: "CODEX", value: "sk-codex", sensitive: true },
      ]);
      assert.strictEqual(spawner.invocations.length, 1);
    }).pipe(
      Effect.provide(
        resolverLayer(spawner.layer, {
          [HOME_PROVIDER]: onePasswordProvider,
          [VAULT_PROVIDER]: openBaoProvider(),
        }),
      ),
    );
  });
});

describe("ProviderSecretResolverLive with OpenBao over the HTTP API", () => {
  const apiProvider = openBaoProvider;

  it.effect("reads the KV v2 field with the stored token and namespace", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "", code: 0 }));
    const http = recordingHttpClient(200, { data: { data: { api_key: "sk-api", port: 8200 } } });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(apiRead());

      assert.deepStrictEqual(resolved, {
        variables: [{ name: "CODEX_TOKEN", value: "sk-api", sensitive: true }],
        unresolved: [],
      });
      assert.deepStrictEqual(http.requests, [
        {
          url: `${BAO_ADDRESS}/v1/kv/data/agents/codex`,
          headers: { "x-vault-token": "s.configured", "x-vault-namespace": "team" },
        },
      ]);
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: apiProvider({ namespace: "team", token: "s.configured" }) },
          http.client,
        ),
      ),
    );
  });

  it.effect("joins a trailing slash address and encodes unsafe path characters", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "", code: 0 }));
    const http = recordingHttpClient(200, { data: { data: { api_key: "x" } } });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.resolve(apiRead("my team/co dex?#api_key"));

      assert.deepStrictEqual(
        http.requests.map((request) => [request.url, "x-vault-namespace" in request.headers]),
        [[`${BAO_ADDRESS}/v1/kv/data/my%20team/co%20dex%3F`, false]],
      );
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: apiProvider({ token: "s.t", address: `${BAO_ADDRESS}/` }) },
          http.client,
        ),
      ),
    );
  });

  it.effect("reports a stored-token provider with no token unresolved without a request", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "", code: 0 }));
    const http = recordingHttpClient(200, { data: { data: { api_key: "sk-api" } } });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(apiRead());

      assert.deepStrictEqual(resolved, { variables: [], unresolved: ["CODEX_TOKEN"] });
      assert.strictEqual(http.requests.length, 0);
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        resolverLayer(spawner.layer, { [VAULT_PROVIDER]: apiProvider({ token: "" }) }, http.client),
      ),
    );
  });

  it.effect("uses BAO_TOKEN from the server environment when auth is env", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "", code: 0 }));
    const http = recordingHttpClient(200, { data: { data: { api_key: "sk-api" } } });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.resolve(apiRead());

      assert.deepStrictEqual(
        http.requests.map((request) => request.headers["x-vault-token"]),
        ["s.from-env"],
      );
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: apiProvider({ auth: "env", token: "s.ignored" }) },
          http.client,
        ).pipe(Layer.provide(hostLayer({ BAO_TOKEN: "s.from-env", VAULT_TOKEN: "s.other" }))),
      ),
    );
  });

  it.effect("reports auth env with BAO_TOKEN unset unresolved without a request", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "", code: 0 }));
    const http = recordingHttpClient(200, { data: { data: { api_key: "sk-api" } } });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(apiRead());

      assert.deepStrictEqual(resolved, { variables: [], unresolved: ["CODEX_TOKEN"] });
      assert.strictEqual(http.requests.length, 0);
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: apiProvider({ auth: "env" }) },
          http.client,
        ).pipe(Layer.provide(hostLayer({ VAULT_TOKEN: "s.other" }))),
      ),
    );
  });

  it.effect("uses the output of `bao print token` when auth is cli", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "s.from-cli\n", stderr: "", code: 0 }));
    const http = recordingHttpClient(200, { data: { data: { api_key: "sk-api" } } });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(apiRead());

      assert.deepStrictEqual(resolved, {
        variables: [{ name: "CODEX_TOKEN", value: "sk-api", sensitive: true }],
        unresolved: [],
      });
      assert.deepStrictEqual(spawner.commands, ["bao"]);
      assert.deepStrictEqual(spawner.invocations, [["print", "token"]]);
      assert.deepStrictEqual(spawner.environments, [
        {
          BAO_ADDR: BAO_ADDRESS,
          BAO_NAMESPACE: "team",
          BAO_TOKEN: undefined,
          VAULT_TOKEN: undefined,
        },
      ]);
      assert.deepStrictEqual(
        http.requests.map((request) => request.headers["x-vault-token"]),
        ["s.from-cli"],
      );
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: apiProvider({ auth: "cli", namespace: "team", token: "s.ignored" }) },
          http.client,
        ).pipe(Layer.provide(hostLayer({ BAO_TOKEN: "s.from-env", VAULT_TOKEN: "s.other" }))),
      ),
    );
  });

  it.effect("reports a failing `bao print token` unresolved without a request", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "no token found", code: 2 }));
    const http = recordingHttpClient(200, { data: { data: { api_key: "sk-api" } } });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(apiRead());

      assert.deepStrictEqual(resolved, { variables: [], unresolved: ["CODEX_TOKEN"] });
      assert.strictEqual(http.requests.length, 0);
      assert.deepStrictEqual(spawner.invocations, [["print", "token"]]);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: apiProvider({ auth: "cli" }) },
          http.client,
        ),
      ),
    );
  });

  it.effect("reports a 403 unresolved", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "", code: 0 }));
    const http = recordingHttpClient(403, { errors: ["permission denied"] });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(apiRead());

      assert.deepStrictEqual(resolved, { variables: [], unresolved: ["CODEX_TOKEN"] });
      assert.strictEqual(http.requests.length, 1);
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: apiProvider({ token: "s.t" }) },
          http.client,
        ),
      ),
    );
  });

  it.effect("reports a missing or non-scalar field unresolved", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "", code: 0 }));
    const http = recordingHttpClient(200, { data: { data: { nested: { a: 1 } } } });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          namedVariable("MISSING", VAULT_PROVIDER, "agents/codex#api_key"),
          namedVariable("OBJECT", VAULT_PROVIDER, "agents/codex#nested"),
        ]),
      );

      assert.deepStrictEqual(Array.from(resolved.unresolved), ["MISSING", "OBJECT"]);
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: apiProvider({ token: "s.t" }) },
          http.client,
        ),
      ),
    );
  });

  it.effect("invalidating one provider keeps the other providers' secrets cached", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "sk-home-token", stderr: "", code: 0 }));
    const http = recordingHttpClient(200, { data: { data: { api_key: "sk-api" } } });
    const environment = decodeEnvironment([
      onePasswordVariable("LEGACY", TOKEN_REFERENCE, HOME_ACCOUNT),
      namedVariable("HOME", HOME_PROVIDER, SECOND_REFERENCE),
      namedVariable("VAULT", VAULT_PROVIDER, "agents/codex#api_key"),
    ]);
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.resolve(environment);
      assert.strictEqual(spawner.invocations.length, 2);
      assert.strictEqual(http.requests.length, 1);

      // An OpenBao edit must not cost a 1Password prompt, legacy or named.
      yield* resolver.invalidate(new Set([VAULT_PROVIDER]));
      yield* resolver.resolve(environment);
      assert.strictEqual(spawner.invocations.length, 2);
      assert.strictEqual(http.requests.length, 2);

      yield* resolver.invalidate(new Set([HOME_PROVIDER]));
      yield* resolver.resolve(environment);
      assert.strictEqual(spawner.invocations.length, 3);
      assert.strictEqual(http.requests.length, 2);
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [HOME_PROVIDER]: onePasswordProvider, [VAULT_PROVIDER]: apiProvider({ token: "s.t" }) },
          http.client,
        ),
      ),
    );
  });

  it.effect("reports an unresponsive address unresolved instead of holding the start", () => {
    const spawner = scriptedOpSpawner(() => ({ stdout: "", stderr: "", code: 0 }));
    const http = HttpClient.make(() => Effect.never);
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const pending = yield* resolver.resolve(apiRead()).pipe(Effect.forkScoped);
      yield* Effect.yieldNow;
      yield* TestClock.adjust("10 seconds");
      const resolved = yield* Fiber.join(pending);

      assert.deepStrictEqual(resolved, { variables: [], unresolved: ["CODEX_TOKEN"] });
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(
      Effect.scoped,
      Effect.provide(
        resolverLayer(spawner.layer, { [VAULT_PROVIDER]: apiProvider({ token: "s.t" }) }, http),
      ),
    );
  });
});

const AWS_PROVIDER = SecretProviderId.make("prod-aws");

const awsProvider = (overrides: { region?: string; profile?: string } = {}) => ({
  kind: "aws-secrets-manager" as const,
  label: "Production",
  region: "us-east-1",
  ...overrides,
});

const awsRead = (reference: string) =>
  decodeEnvironment([namedVariable("OPENAI_API_KEY", AWS_PROVIDER, reference)]);

const awsBody = (body: { SecretString?: string; SecretBinary?: string }) =>
  JSON.stringify({ ARN: "arn:aws:secretsmanager:us-east-1:000000000000:secret:prod", ...body });

describe("ProviderSecretResolverLive with AWS Secrets Manager", () => {
  it.effect("reads the whole secret string in the provider's region", () => {
    const spawner = scriptedOpSpawner(() => ({
      stdout: awsBody({ SecretString: "sk-aws-token" }),
      stderr: "",
      code: 0,
    }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(awsRead("prod/openai"));

      assert.deepStrictEqual(resolved, {
        variables: [{ name: "OPENAI_API_KEY", value: "sk-aws-token", sensitive: true }],
        unresolved: [],
      });
      assert.deepStrictEqual(spawner.commands, ["aws"]);
      assert.deepStrictEqual(spawner.invocations, [
        [
          "secretsmanager",
          "get-secret-value",
          "--secret-id",
          "prod/openai",
          "--region",
          "us-east-1",
          "--output",
          "json",
          "--no-cli-pager",
          "--no-cli-auto-prompt",
        ],
      ]);
    }).pipe(Effect.provide(resolverLayer(spawner.layer, { [AWS_PROVIDER]: awsProvider() })));
  });

  it.effect("passes the named profile when one is set", () => {
    const spawner = scriptedOpSpawner(() => ({
      stdout: awsBody({ SecretString: "sk-aws-token" }),
      stderr: "",
      code: 0,
    }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      yield* resolver.resolve(awsRead("prod/openai"));

      assert.deepStrictEqual(spawner.invocations, [
        [
          "secretsmanager",
          "get-secret-value",
          "--secret-id",
          "prod/openai",
          "--region",
          "eu-west-2",
          "--profile",
          "production",
          "--output",
          "json",
          "--no-cli-pager",
          "--no-cli-auto-prompt",
        ],
      ]);
    }).pipe(
      Effect.provide(
        resolverLayer(spawner.layer, {
          [AWS_PROVIDER]: awsProvider({ region: "eu-west-2", profile: "production" }),
        }),
      ),
    );
  });

  it.effect("reads one key of a JSON secret", () => {
    const spawner = scriptedOpSpawner(() => ({
      stdout: awsBody({
        SecretString: JSON.stringify({ api_key: "sk-aws-key", retries: 3, strict: true }),
      }),
      stderr: "",
      code: 0,
    }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          namedVariable("KEY", AWS_PROVIDER, "prod/openai#api_key"),
          namedVariable("RETRIES", AWS_PROVIDER, "prod/openai#retries"),
          namedVariable("STRICT", AWS_PROVIDER, "prod/openai#strict"),
        ]),
      );

      assert.deepStrictEqual(resolved, {
        variables: [
          { name: "KEY", value: "sk-aws-key", sensitive: true },
          { name: "RETRIES", value: "3", sensitive: true },
          { name: "STRICT", value: "true", sensitive: true },
        ],
        unresolved: [],
      });
      assert.strictEqual(spawner.invocations[0]?.[3], "prod/openai");
    }).pipe(Effect.provide(resolverLayer(spawner.layer, { [AWS_PROVIDER]: awsProvider() })));
  });

  it.effect("reports a missing, non-scalar, or non-object key unresolved", () => {
    const spawner = scriptedOpSpawner((args) => ({
      stdout: awsBody({
        SecretString: args.includes("prod/plain")
          ? "not json"
          : JSON.stringify({ nested: { a: 1 }, list: [1] }),
      }),
      stderr: "",
      code: 0,
    }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          namedVariable("MISSING", AWS_PROVIDER, "prod/openai#absent"),
          namedVariable("NESTED", AWS_PROVIDER, "prod/openai#nested"),
          namedVariable("LIST", AWS_PROVIDER, "prod/openai#list"),
          namedVariable("PLAIN", AWS_PROVIDER, "prod/plain#api_key"),
        ]),
      );

      assert.deepStrictEqual(resolved, {
        variables: [],
        unresolved: ["MISSING", "NESTED", "LIST", "PLAIN"],
      });
    }).pipe(Effect.provide(resolverLayer(spawner.layer, { [AWS_PROVIDER]: awsProvider() })));
  });

  it.effect("reports a binary-only secret unresolved", () => {
    const spawner = scriptedOpSpawner(() => ({
      stdout: awsBody({ SecretBinary: "aGVsbG8=" }),
      stderr: "",
      code: 0,
    }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(awsRead("prod/blob"));

      assert.deepStrictEqual(resolved, { variables: [], unresolved: ["OPENAI_API_KEY"] });
    }).pipe(Effect.provide(resolverLayer(spawner.layer, { [AWS_PROVIDER]: awsProvider() })));
  });

  it.effect("reports a failing `aws` unresolved and logs stderr but never stdout", () => {
    const logged: Array<string> = [];
    const logger = Logger.make(({ message }) => {
      logged.push(JSON.stringify(message));
    });
    const spawner = scriptedOpSpawner(() => ({
      stdout: awsBody({ SecretString: "sk-must-not-be-logged" }),
      stderr: "Error when retrieving token from sso: Token has expired",
      code: 254,
    }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(awsRead("prod/openai"));

      assert.deepStrictEqual(resolved, { variables: [], unresolved: ["OPENAI_API_KEY"] });
      const output = logged.join("\n");
      assert.include(output, "Could not read provider secret from AWS Secrets Manager");
      assert.include(output, "Token has expired");
      assert.notInclude(output, "sk-must-not-be-logged");
    }).pipe(
      Effect.provide(
        Layer.mergeAll(
          resolverLayer(spawner.layer, { [AWS_PROVIDER]: awsProvider() }),
          Logger.layer([logger]),
        ),
      ),
    );
  });

  it.effect("refuses a flag-shaped secret id and an empty key without spawning", () => {
    const spawner = scriptedOpSpawner(() => ({
      stdout: awsBody({ SecretString: "sk-aws-token" }),
      stderr: "",
      code: 0,
    }));
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const resolved = yield* resolver.resolve(
        decodeEnvironment([
          namedVariable("FLAG", AWS_PROVIDER, "--profile=evil"),
          namedVariable("FLAG_KEY", AWS_PROVIDER, "-x#api_key"),
          namedVariable("EMPTY_KEY", AWS_PROVIDER, "prod/openai#"),
          namedVariable("NO_ID", AWS_PROVIDER, "#api_key"),
        ]),
      );

      assert.deepStrictEqual(Array.from(resolved.unresolved), [
        "FLAG",
        "FLAG_KEY",
        "EMPTY_KEY",
        "NO_ID",
      ]);
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(Effect.provide(resolverLayer(spawner.layer, { [AWS_PROVIDER]: awsProvider() })));
  });
});

const sourceOf = (variable: Parameters<typeof decodeEnvironment>[0][number]) => {
  const [decoded] = decodeEnvironment([variable]);
  const source = decoded?.value;
  if (source === undefined || typeof source === "string") {
    throw new Error("expected a secret source");
  }
  return source;
};

describe("ProviderSecretResolver.check", () => {
  it.effect("reports a legacy 1Password read as resolved and primes the cache", () => {
    const spawner = recordingOpSpawner({ stdout: "sk-live-token", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;
      const variable = onePasswordVariable("TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT);

      const result = yield* resolver.check(sourceOf(variable));

      assert.deepStrictEqual(result, { _tag: "Resolved" });
      assert.strictEqual(spawner.invocations.length, 1);
      const resolved = yield* resolver.resolve(decodeEnvironment([variable]));
      assert.deepStrictEqual(resolved.unresolved, []);
      assert.strictEqual(spawner.invocations.length, 1);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("reports the op error text when 1Password exits non-zero", () => {
    const spawner = recordingOpSpawner({
      stdout: "",
      stderr: "[ERROR] could not read secret: item not found\n",
      code: 1,
    });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const result = yield* resolver.check(
        sourceOf(onePasswordVariable("TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT)),
      );

      assert.deepStrictEqual(result, {
        _tag: "Failed",
        detail: "[ERROR] could not read secret: item not found",
      });
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("reports a named provider that is not in settings", () => {
    const spawner = recordingOpSpawner({ stdout: "unused", stderr: "", code: 0 });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const result = yield* resolver.check(
        sourceOf(namedVariable("TOKEN", VAULT_PROVIDER, "agents/codex#api_key")),
      );

      assert.deepStrictEqual(result, {
        _tag: "Failed",
        detail: "Secret provider is not configured",
      });
      assert.strictEqual(spawner.invocations.length, 0);
    }).pipe(Effect.provide(resolverLayer(spawner.layer)));
  });

  it.effect("reports the OpenBao errors on a 403", () => {
    const spawner = recordingOpSpawner({ stdout: "", stderr: "", code: 0 });
    const http = recordingHttpClient(403, { errors: ["permission denied"] });
    return Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const result = yield* resolver.check(
        sourceOf(namedVariable("TOKEN", VAULT_PROVIDER, "agents/codex#api_key")),
      );

      assert.deepStrictEqual(result, { _tag: "Failed", detail: "permission denied" });
    }).pipe(
      Effect.provide(
        resolverLayer(
          spawner.layer,
          { [VAULT_PROVIDER]: openBaoProvider({ token: "s.t" }) },
          http.client,
        ),
      ),
    );
  });

  it.effect("fails without a secret store on the default resolver", () =>
    Effect.gen(function* () {
      const resolver = yield* ProviderSecretResolver;

      const result = yield* resolver.check(
        sourceOf(onePasswordVariable("TOKEN", TOKEN_REFERENCE, HOME_ACCOUNT)),
      );

      assert.deepStrictEqual(result, {
        _tag: "Failed",
        detail: "Secret stores are not available on this server",
      });
    }),
  );
});
