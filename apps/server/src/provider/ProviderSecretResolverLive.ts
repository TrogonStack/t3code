// @effect-diagnostics nodeBuiltinImport:off -- the op inject separator only needs a random token at this subprocess boundary
/**
 * ProviderSecretResolverLive: implementation of `ProviderSecretResolver` that
 * reads each secret store through its HTTP API or its own CLI.
 *
 * A named secret provider is looked up in `settings.secretProviders` when its
 * secret is read, not when the reference is collected, so the reference stays
 * a pure value and the provider's current configuration is what gets used.
 * Removing a provider leaves its references unresolved rather than failing.
 *
 * OpenBao reads the KV v2 secret over its HTTP API:
 * `GET <address>/v1/<mount>/data/<path>` with the token in `X-Vault-Token`.
 * The provider's `auth` names the one place the token comes from, with no
 * fallback to another: `token` is the one stored on the provider, `env` is the
 * server process's `BAO_TOKEN`, and `cli` is the output of `bao print token`,
 * so the CLI's own login applies, the same way `op` uses the user's existing
 * session.
 *
 * AWS Secrets Manager reads `aws secretsmanager get-secret-value` in the
 * provider's region, with its named profile when one is set. No credentials
 * are stored: the CLI resolves them from that profile or its default chain
 * (environment, default profile, SSO, instance role). A reference is
 * `name-or-arn` for the whole secret string or `name-or-arn#key` for one key
 * of a JSON secret. Binary secrets are not supported.
 *
 * 1Password resolution is `op read --account <account> <reference>`, which is the same
 * command the user would run by hand and inherits their existing `op` session,
 * so there is no second place to configure credentials. The account is what
 * keeps a user signed into several 1Password accounts from reading against
 * whichever one `op` picks by default. Reads run one at a time: two
 * concurrent reads against a locked vault stack up two biometric prompts.
 *
 * The prompt is charged per `op` invocation rather than per secret, so reading
 * one reference at a time makes the whole fleet cost one authorization each.
 * `prime` exists for that: `op inject` substitutes any number of references in
 * a single process, so the caller that is about to build every instance pays
 * one prompt per account for all of them.
 *
 * Failures are cached alongside successes. If the vault is locked when the
 * first thread starts, every later thread in that session would otherwise
 * re-prompt; caching the miss keeps the failure quiet and puts recovery on
 * the refresh button, which is where the user already looks when a provider
 * shows as logged out.
 *
 * @module provider/ProviderSecretResolverLive
 */
import {
  type AwsSecretsManagerSecretProviderConfig,
  OnePasswordAccount,
  OnePasswordAccountSummary,
  type OpenBaoSecretProviderConfig,
  type SecretProviderId,
} from "@t3tools/contracts";
import * as Cache from "effect/Cache";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as FileSystem from "effect/FileSystem";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";
import * as NodeCrypto from "node:crypto";
import * as HostProcess from "@t3tools/shared/HostProcess";
import { HttpClient, HttpClientRequest } from "effect/http";
import { ChildProcess, ChildProcessSpawner } from "effect/process";
import { resolveSpawnCommand } from "@t3tools/shared/shell";

import type { ResolvedProviderEnvironmentVariable } from "@t3tools/provider-core/server/instanceEnvironment";
import {
  OnePasswordSecretReference,
  providerSecretReference,
  type NamedProviderSecretReference,
  type ProviderSecretReference,
} from "./ProviderSecretReference.ts";
import * as Settings from "../serverSettings.ts";
import { spawnAndCollect } from "@t3tools/provider-core/server/snapshotProbe";
import {
  ProviderSecretResolver,
  type ProviderSecretResolverShape,
} from "./ProviderSecretResolver.ts";

/**
 * What one read of a secret store came back with. The failure carries the
 * store's own reason so `check` can show it; `resolve` only needs to know
 * whether there was a value. Details are the store's error text and never
 * include a secret.
 */
type SecretRead =
  | { readonly _tag: "Read"; readonly value: string }
  | { readonly _tag: "Unreadable"; readonly detail: string };

const readValue = (value: string): SecretRead => ({ _tag: "Read", value });
const unreadable = (detail: string): SecretRead => ({ _tag: "Unreadable", detail });
const readNonEmpty = (value: string | undefined, emptyDetail: string) =>
  value === undefined || value.length === 0 ? unreadable(emptyDetail) : readValue(value);
const exitDetail = (result: { readonly code: number; readonly stderr: string }) => {
  const stderr = result.stderr.trim();
  return stderr.length > 0 ? stderr : `Exited with code ${result.code}`;
};

const ONE_PASSWORD_BINARY = "op";
const OPENBAO_BINARY = "bao";
const AWS_BINARY = "aws";

/**
 * Every `op` call skips its cache. The cache lives in an `op daemon` that `op`
 * spawns on its own, which hangs on macOS permission prompts nobody can see
 * from a background server and leaks zombie processes on Linux. Reads are
 * already held in memory here, so the daemon only adds risk.
 */
const resolveOnePasswordCommand = (args: ReadonlyArray<string>) =>
  resolveSpawnCommand(ONE_PASSWORD_BINARY, ["--cache=false", ...args]);

/**
 * Bound on a single `op read`. Long enough for a user to reach for the
 * fingerprint reader, short enough that a vault that will never answer does
 * not wedge the instance rebuild that is waiting on it.
 */
const SECRET_READ_TIMEOUT = Duration.seconds(45);
/** OpenBao never prompts, so an unreachable address should not hold a start for long. */
const OPENBAO_REQUEST_TIMEOUT = Duration.seconds(10);

/**
 * References are small and few - one per provider credential. The cap only
 * exists so a settings file that somehow accumulates hundreds cannot pin
 * every secret it ever mentioned in memory.
 */
const SECRET_CACHE_CAPACITY = 64;

/**
 * Read many references from one account in one `op inject`.
 *
 * `op inject` substitutes references inside a template, so the template is the
 * references themselves joined by a separator, and the output is the secrets
 * in the same order. The separator is random per call because a secret can
 * contain anything at all, newlines included: splitting on a fixed marker
 * would let a secret that happened to contain it shift every later value.
 *
 * Returns `undefined` when the batch cannot be trusted as a whole, which
 * includes a single bad reference, since `op` resolves the template or fails
 * it. The caller treats that as "not primed" and reads one at a time, which is
 * both the per-variable failure isolation and the way the user finds out which
 * reference is the broken one.
 *
 * The template goes to `op` through a scoped temp file rather than stdin.
 * `op` only reads piped input from a named pipe, and a child spawned from Node
 * is handed a socket pair, so a piped template is never seen and the batch
 * fails with "expected data on stdin but none found" every single time. The
 * file holds only `op://` references, which already sit in settings in plain
 * text; the secrets themselves come back on stdout and never touch disk.
 */
const readSecretsTogether = Effect.fn("readSecretsTogether")(function* (
  account: OnePasswordAccount,
  references: ReadonlyArray<string>,
) {
  const fileSystem = yield* FileSystem.FileSystem;
  const separator = `__t3-secret-${NodeCrypto.randomUUID()}__`;
  const template = references.map((reference) => `{{ ${reference} }}`).join(separator);
  const templatePath = yield* fileSystem.makeTempFileScoped({ prefix: "t3code-op-inject-" });
  yield* fileSystem.writeFileString(templatePath, template);
  const spawnCommand = yield* resolveOnePasswordCommand([
    "inject",
    "--account",
    account,
    "-i",
    templatePath,
  ]);
  const result = yield* spawnAndCollect(
    ONE_PASSWORD_BINARY,
    ChildProcess.make(spawnCommand.command, spawnCommand.args, {
      shell: spawnCommand.shell,
    }),
  );
  if (result.code !== 0) {
    // `op` names the reference it could not resolve on stderr and never echoes
    // a secret, so this is safe to log verbatim.
    yield* Effect.logWarning("Could not batch-read provider secrets from 1Password", {
      account,
      references: references.length,
      exitCode: result.code,
      detail: result.stderr.trim(),
    });
    return undefined;
  }
  const values = result.stdout.split(separator);
  if (values.length !== references.length) {
    yield* Effect.logWarning("1Password returned an unexpected number of provider secrets", {
      expected: references.length,
      received: values.length,
    });
    return undefined;
  }
  return values.map((value) => {
    const secret = value.trim();
    return secret.length > 0 ? secret : undefined;
  });
});

const readOnePasswordSecret = Effect.fn("readOnePasswordSecret")(function* ({
  reference,
  account,
}: OnePasswordSecretReference) {
  const spawnCommand = yield* resolveOnePasswordCommand([
    "read",
    "--account",
    account,
    "--no-newline",
    reference,
  ]);
  const result = yield* spawnAndCollect(
    ONE_PASSWORD_BINARY,
    ChildProcess.make(spawnCommand.command, spawnCommand.args, { shell: spawnCommand.shell }),
  );
  if (result.code !== 0) {
    // `op` reports "not signed in", "item not found", and friends on stderr
    // and never echoes the secret itself, so this is safe to log verbatim.
    yield* Effect.logWarning("Could not read provider secret from 1Password", {
      reference,
      account,
      exitCode: result.code,
      detail: result.stderr.trim(),
    });
    return unreadable(exitDetail(result));
  }
  return readNonEmpty(result.stdout.trim(), "1Password returned an empty value");
});

const OnePasswordAccountListJson = Schema.fromJsonString(
  Schema.Array(Schema.Struct({ url: OnePasswordAccount, email: Schema.String })),
);

const readOnePasswordAccounts = Effect.fn("readOnePasswordAccounts")(function* () {
  const spawnCommand = yield* resolveOnePasswordCommand(["account", "list", "--format", "json"]);
  const result = yield* spawnAndCollect(
    ONE_PASSWORD_BINARY,
    ChildProcess.make(spawnCommand.command, spawnCommand.args, { shell: spawnCommand.shell }),
  );
  if (result.code !== 0) {
    yield* Effect.logWarning("Could not list 1Password accounts", {
      exitCode: result.code,
      detail: result.stderr.trim(),
    });
    return [];
  }
  const accounts = yield* Schema.decodeUnknownEffect(OnePasswordAccountListJson)(result.stdout);
  return accounts.map(({ url, email }): OnePasswordAccountSummary => ({ account: url, email }));
});

/**
 * Split an OpenBao reference into the secret's path inside the mount and the
 * key to print. The path is handed to `bao` as a positional argument, so one
 * starting with `-` would be read as a flag and is refused.
 */
function parseOpenBaoReference(reference: string) {
  const separator = reference.indexOf("#");
  if (separator === -1) {
    return undefined;
  }
  const path = reference.slice(0, separator);
  const field = reference.slice(separator + 1);
  if (path.length === 0 || field.length === 0 || path.startsWith("-")) {
    return undefined;
  }
  return { path, field };
}

const OpenBaoErrorBody = Schema.Struct({ errors: Schema.optional(Schema.Array(Schema.String)) });
const OpenBaoSecretBody = Schema.Struct({
  data: Schema.Struct({ data: Schema.Record(Schema.String, Schema.Unknown) }),
});
const decodeOpenBaoErrorBody = Schema.decodeUnknownOption(OpenBaoErrorBody);
const decodeOpenBaoSecretBody = Schema.decodeUnknownOption(OpenBaoSecretBody);

const nonEmpty = (value: string | undefined) =>
  value === undefined || value.length === 0 ? undefined : value;

/**
 * The token the API read authenticates with, from the one source the
 * provider's `auth` names. `cli` asks `bao print token`, which owns where its
 * login lives, so nothing here reads a token file or a `VAULT_*` variable, and
 * the token variables are withheld from that child so it reports the login
 * rather than the server's environment.
 * Returns the reason, after logging it, when that source has no token.
 */
const resolveOpenBaoToken = Effect.fn("resolveOpenBaoToken")(function* (
  provider: SecretProviderId,
  config: OpenBaoSecretProviderConfig,
) {
  switch (config.auth) {
    case "token": {
      const token = nonEmpty(config.token);
      if (token === undefined) {
        yield* Effect.logWarning("OpenBao provider has no stored token", { provider });
        return unreadable("OpenBao provider has no stored token");
      }
      return readValue(token);
    }
    case "env": {
      const env = yield* HostProcess.Environment;
      const token = nonEmpty(env.BAO_TOKEN);
      if (token === undefined) {
        yield* Effect.logWarning("BAO_TOKEN is not set in the server environment", { provider });
        return unreadable("BAO_TOKEN is not set in the server environment");
      }
      return readValue(token);
    }
    case "cli": {
      const spawnCommand = yield* resolveSpawnCommand(OPENBAO_BINARY, ["print", "token"]);
      const result = yield* spawnAndCollect(
        OPENBAO_BINARY,
        ChildProcess.make(spawnCommand.command, spawnCommand.args, {
          shell: spawnCommand.shell,
          env: {
            BAO_ADDR: config.address,
            ...(config.namespace === undefined ? {} : { BAO_NAMESPACE: config.namespace }),
            // `bao` prefers these over its saved login, which would turn the
            // CLI policy back into the environment one. Node drops a key whose
            // value is undefined, so the child never sees them.
            BAO_TOKEN: undefined,
            VAULT_TOKEN: undefined,
          },
          extendEnv: true,
        }),
      );
      const token = result.code === 0 ? nonEmpty(result.stdout.trim()) : undefined;
      if (token === undefined) {
        // `bao print token` reports a missing login on stderr and never echoes
        // the token there, so this is safe to log verbatim.
        yield* Effect.logWarning("Could not read an OpenBao token from the bao CLI", {
          provider,
          exitCode: result.code,
          detail: result.stderr.trim(),
        });
        return unreadable(exitDetail(result));
      }
      return readValue(token);
    }
  }
});

/** Percent-encode each path segment and keep `/` as the separator. */
const encodeOpenBaoPath = (path: string) => path.split("/").map(encodeURIComponent).join("/");

const readOpenBaoSecretOverApi = Effect.fn("readOpenBaoSecretOverApi")(
  function* (
    provider: SecretProviderId,
    config: OpenBaoSecretProviderConfig,
    location: { readonly path: string; readonly field: string },
    reference: string,
  ) {
    const token = yield* resolveOpenBaoToken(provider, config);
    if (token._tag === "Unreadable") {
      return token;
    }
    const client = yield* HttpClient.HttpClient;
    const url = `${config.address.replace(/\/+$/, "")}/v1/${encodeOpenBaoPath(config.mount)}/data/${encodeOpenBaoPath(location.path)}`;
    const request = HttpClientRequest.get(url).pipe(
      HttpClientRequest.setHeaders({
        "X-Vault-Token": token.value,
        ...(config.namespace === undefined || config.namespace.length === 0
          ? {}
          : { "X-Vault-Namespace": config.namespace }),
      }),
    );
    // Trace headers would tell the vault which of our spans made the request.
    const response = yield* client
      .execute(request)
      .pipe(
        Effect.provideService(HttpClient.TracerPropagationEnabled, false),
        Effect.timeout(OPENBAO_REQUEST_TIMEOUT),
      );
    const body: unknown = yield* response.json.pipe(Effect.orElseSucceed(() => undefined));
    if (response.status < 200 || response.status >= 300) {
      // OpenBao's `errors` array names the failure and never carries a secret.
      const errors = decodeOpenBaoErrorBody(body).pipe(
        Option.flatMap((decoded) => Option.fromNullishOr(decoded.errors)),
        Option.getOrElse((): ReadonlyArray<string> => []),
      );
      yield* Effect.logWarning("Could not read provider secret from OpenBao", {
        provider,
        reference,
        status: response.status,
        errorCount: errors.length,
      });
      return unreadable(errors.length > 0 ? errors.join("; ") : `HTTP ${response.status}`);
    }
    const value = decodeOpenBaoSecretBody(body).pipe(
      Option.map((decoded) => decoded.data.data[location.field]),
      Option.getOrUndefined,
    );
    const secret =
      typeof value === "string"
        ? value
        : typeof value === "number" || typeof value === "boolean"
          ? String(value)
          : undefined;
    if (secret === undefined || secret.length === 0) {
      yield* Effect.logWarning("OpenBao secret has no usable value for the requested field", {
        provider,
        reference,
        status: response.status,
      });
      return unreadable("OpenBao secret has no usable value for the requested field");
    }
    return readValue(secret);
  },
  (effect, provider, _config, _location, reference) =>
    effect.pipe(
      Effect.catch((error) =>
        Effect.logWarning("Could not read provider secret from OpenBao", {
          provider,
          reference,
          detail: String(error),
        }).pipe(Effect.as(unreadable(String(error)))),
      ),
    ),
);

const readOpenBaoSecret = Effect.fn("readOpenBaoSecret")(function* (
  provider: SecretProviderId,
  config: OpenBaoSecretProviderConfig,
  reference: string,
) {
  const location = parseOpenBaoReference(reference);
  if (location === undefined) {
    yield* Effect.logWarning("OpenBao secret reference must be path/to/secret#field", {
      provider,
      reference,
    });
    return unreadable("OpenBao secret reference must be path/to/secret#field");
  }
  return yield* readOpenBaoSecretOverApi(provider, config, location, reference);
});

/**
 * Split an AWS Secrets Manager reference into the secret id and the optional
 * JSON key. The id is handed to `aws` as the value of `--secret-id`, so one
 * starting with `-` would be read as a flag and is refused. A secret name
 * cannot contain `#`, so the first one always starts the key.
 */
function parseAwsReference(reference: string) {
  const separator = reference.indexOf("#");
  const secretId = separator === -1 ? reference : reference.slice(0, separator);
  const key = separator === -1 ? undefined : reference.slice(separator + 1);
  if (secretId.length === 0 || secretId.startsWith("-") || key === "") {
    return undefined;
  }
  return { secretId, key };
}

const AwsSecretValueJson = Schema.fromJsonString(
  Schema.Struct({
    SecretString: Schema.optional(Schema.String),
    SecretBinary: Schema.optional(Schema.String),
  }),
);
const decodeAwsSecretValue = Schema.decodeUnknownOption(AwsSecretValueJson);
const decodeJson = Schema.decodeUnknownOption(Schema.fromJsonString(Schema.Unknown));

const readAwsSecret = Effect.fn("readAwsSecret")(function* (
  provider: SecretProviderId,
  config: AwsSecretsManagerSecretProviderConfig,
  reference: string,
) {
  const location = parseAwsReference(reference);
  if (location === undefined) {
    yield* Effect.logWarning(
      "AWS Secrets Manager reference must be name-or-arn or name-or-arn#key",
      { provider, reference },
    );
    return unreadable("AWS Secrets Manager reference must be name-or-arn or name-or-arn#key");
  }
  const spawnCommand = yield* resolveSpawnCommand(AWS_BINARY, [
    "secretsmanager",
    "get-secret-value",
    "--secret-id",
    location.secretId,
    "--region",
    config.region,
    ...(config.profile === undefined ? [] : ["--profile", config.profile]),
    "--output",
    "json",
    "--no-cli-pager",
    "--no-cli-auto-prompt",
  ]);
  const result = yield* spawnAndCollect(
    AWS_BINARY,
    ChildProcess.make(spawnCommand.command, spawnCommand.args, { shell: spawnCommand.shell }),
  );
  if (result.code !== 0) {
    // `aws` reports missing credentials, expired SSO sessions, and
    // ResourceNotFound on stderr and never echoes the secret, so this is safe
    // to log verbatim. Stdout carries the secret and is never logged.
    yield* Effect.logWarning("Could not read provider secret from AWS Secrets Manager", {
      provider,
      reference,
      exitCode: result.code,
      detail: result.stderr.trim(),
    });
    return unreadable(exitDetail(result));
  }
  const body = Option.getOrUndefined(decodeAwsSecretValue(result.stdout));
  if (body === undefined) {
    yield* Effect.logWarning("AWS Secrets Manager returned an unexpected response", {
      provider,
      reference,
    });
    return unreadable("AWS Secrets Manager returned an unexpected response");
  }
  if (body.SecretString === undefined) {
    yield* Effect.logWarning("AWS Secrets Manager binary secrets are not supported", {
      provider,
      reference,
    });
    return unreadable("AWS Secrets Manager binary secrets are not supported");
  }
  if (location.key === undefined) {
    return readNonEmpty(body.SecretString, "The secret string is empty");
  }
  const parsed = Option.getOrUndefined(decodeJson(body.SecretString));
  const value =
    typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
      ? Reflect.get(parsed, location.key)
      : undefined;
  if (typeof value === "string") {
    return readNonEmpty(value, "The value for the key is empty");
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return readValue(String(value));
  }
  yield* Effect.logWarning("AWS Secrets Manager secret has no scalar value for the key", {
    provider,
    reference,
  });
  return unreadable("The secret has no scalar value for the key");
});

/** Log fields that identify a secret without carrying its value. */
const describeSecret = (secret: ProviderSecretReference) =>
  secret._tag === "1password"
    ? { reference: secret.reference, account: secret.account }
    : { reference: secret.reference, provider: secret.provider };

export const ProviderSecretResolverLive = Layer.effect(
  ProviderSecretResolver,
  Effect.gen(function* () {
    // The service tag declares `prime` as `Effect<void>`, so the spawner and
    // the filesystem it needs are captured here rather than asked of the
    // caller, the same way the cache's own lookup captures the spawner.
    const primeContext = yield* Effect.context<
      ChildProcessSpawner.ChildProcessSpawner | FileSystem.FileSystem
    >();
    const serverSettings = yield* Settings.ServerSettingsService;

    const secretProviderConfig = (provider: SecretProviderId) =>
      serverSettings.getSettings.pipe(
        Effect.map((settings) => settings.secretProviders[provider]),
        Effect.orElseSucceed(() => undefined),
      );

    const readNamedProviderSecret = Effect.fn("readNamedProviderSecret")(function* ({
      provider,
      reference,
    }: NamedProviderSecretReference) {
      const config = yield* secretProviderConfig(provider);
      if (config === undefined) {
        yield* Effect.logWarning("Provider secret names a secret provider that is not configured", {
          provider,
          reference,
        });
        return unreadable("Secret provider is not configured");
      }
      switch (config.kind) {
        case "1password":
          return yield* readOnePasswordSecret(
            new OnePasswordSecretReference({ reference, account: config.account }),
          );
        case "openbao":
          return yield* readOpenBaoSecret(provider, config, reference);
        case "aws-secrets-manager":
          return yield* readAwsSecret(provider, config, reference);
      }
    });

    const readSecret = (secret: ProviderSecretReference) => {
      switch (secret._tag) {
        case "1password":
          return readOnePasswordSecret(secret);
        case "secret-provider":
          return readNamedProviderSecret(secret);
      }
    };

    const readContext =
      yield* Effect.context<Effect.Services<ReturnType<typeof readNamedProviderSecret>>>();

    const readSecretBounded = (reference: ProviderSecretReference) =>
      readSecret(reference).pipe(
        Effect.timeoutOption(SECRET_READ_TIMEOUT),
        Effect.map(Option.getOrElse(() => unreadable("Timed out waiting for the secret store"))),
        Effect.catch((error) =>
          Effect.logWarning("Could not run the secret store CLI to read a provider secret", {
            ...describeSecret(reference),
            detail: String(error),
          }).pipe(Effect.as(unreadable(String(error)))),
        ),
      );

    const cache = yield* Cache.make({
      capacity: SECRET_CACHE_CAPACITY,
      // No time to live: a resolved secret is held until the user asks for a
      // provider refresh. Expiring on a timer would reintroduce the surprise
      // biometric prompt mid-session that the cache exists to remove.
      lookup: (reference: ProviderSecretReference) =>
        readSecretBounded(reference).pipe(
          Effect.map((read) => (read._tag === "Read" ? read.value : undefined)),
        ),
    });

    const check: ProviderSecretResolverShape["check"] = (source) =>
      Effect.gen(function* () {
        const reference = providerSecretReference(source);
        if (reference === undefined) {
          return { _tag: "Failed", detail: "This variable does not name a secret" } as const;
        }
        const read = yield* readSecretBounded(reference).pipe(Effect.provideContext(readContext));
        if (read._tag === "Unreadable") {
          return { _tag: "Failed", detail: read.detail } as const;
        }
        yield* Cache.set(cache, reference, read.value);
        return { _tag: "Resolved" } as const;
      });

    const resolve: ProviderSecretResolverShape["resolve"] = (environment) =>
      Effect.gen(function* () {
        const resolved: Array<ResolvedProviderEnvironmentVariable> = [];
        const unresolved: Array<string> = [];
        for (const { name, value, sensitive } of environment ?? []) {
          const reference = providerSecretReference(value);
          if (reference === undefined) {
            if (typeof value === "string") {
              resolved.push({ name, value, sensitive });
            }
            continue;
          }
          const secret = yield* Cache.get(cache, reference);
          if (secret === undefined) {
            unresolved.push(name);
            continue;
          }
          resolved.push({ name, value: secret, sensitive: true });
        }
        return { variables: resolved, unresolved };
      });

    /**
     * `wanted` maps each `op://` reference to every cache key that reads it, so
     * a legacy source and a named 1Password provider pointing at the same item
     * in the same account share one slot in the batch.
     */
    const primeOnePassword = Effect.fn("primeOnePassword")(function* (
      account: OnePasswordAccount,
      wanted: ReadonlyMap<string, ReadonlyArray<ProviderSecretReference>>,
    ) {
      // One reference costs one prompt whichever command reads it, so there
      // is nothing to save and `op read` gives the better error.
      if (wanted.size < 2) {
        return;
      }
      const references = Array.from(wanted.keys());
      const values = yield* readSecretsTogether(account, references).pipe(
        Effect.scoped,
        Effect.timeoutOption(SECRET_READ_TIMEOUT),
        Effect.map(Option.getOrUndefined),
        Effect.catch((error) =>
          Effect.logWarning("Could not run 1Password to batch-read provider secrets", {
            account,
            references: references.length,
            detail: String(error),
          }).pipe(Effect.as(undefined)),
        ),
      );
      if (values === undefined) {
        return;
      }
      yield* Effect.forEach(
        references,
        (reference, index) =>
          Effect.forEach(
            wanted.get(reference) ?? [],
            (key) => Cache.set(cache, key, values[index]),
            {
              discard: true,
            },
          ),
        { discard: true },
      );
    });

    /**
     * The 1Password account a reference reads from, or `undefined` when it is
     * not a 1Password read. OpenBao has no unlock prompt to amortize, so its
     * references are left for `resolve` to read one at a time.
     */
    const onePasswordAccountOf = (reference: ProviderSecretReference) => {
      switch (reference._tag) {
        case "1password":
          return Effect.succeed(reference.account);
        case "secret-provider":
          return secretProviderConfig(reference.provider).pipe(
            Effect.map((config) => (config?.kind === "1password" ? config.account : undefined)),
          );
      }
    };

    const prime: ProviderSecretResolverShape["prime"] = (references) =>
      Effect.gen(function* () {
        // `op inject` reads every reference in its template from one account,
        // so a batch is one call per account.
        const onePasswordByAccount = new Map<
          OnePasswordAccount,
          Map<string, Array<ProviderSecretReference>>
        >();
        for (const reference of references) {
          if (yield* Cache.has(cache, reference)) {
            continue;
          }
          const account = yield* onePasswordAccountOf(reference);
          if (account === undefined) {
            continue;
          }
          const group = onePasswordByAccount.get(account) ?? new Map();
          const keys = group.get(reference.reference) ?? [];
          keys.push(reference);
          group.set(reference.reference, keys);
          onePasswordByAccount.set(account, group);
        }
        yield* Effect.forEach(
          onePasswordByAccount,
          ([account, wanted]) => primeOnePassword(account, wanted),
          { discard: true },
        );
      }).pipe(Effect.provideContext(primeContext));

    const listOnePasswordAccounts: ProviderSecretResolverShape["listOnePasswordAccounts"] =
      readOnePasswordAccounts().pipe(
        Effect.timeoutOption(SECRET_READ_TIMEOUT),
        Effect.map(Option.getOrElse(() => [])),
        Effect.catch((error) =>
          Effect.logWarning("Could not run 1Password to list accounts", {
            detail: String(error),
          }).pipe(Effect.as([])),
        ),
        Effect.provideContext(primeContext),
      );

    const invalidate: ProviderSecretResolverShape["invalidate"] = (providers) =>
      providers === undefined
        ? Cache.invalidateAll(cache)
        : Effect.gen(function* () {
            const stale = Array.from(yield* Cache.keys(cache)).filter(
              (key) => key._tag === "secret-provider" && providers.has(key.provider),
            );
            yield* Effect.forEach(stale, (key) => Cache.invalidate(cache, key), { discard: true });
          });

    return {
      resolve,
      prime,
      invalidate,
      check,
      listOnePasswordAccounts,
    };
  }),
);
