import { spawnAndCollect } from "@t3tools/provider-core/server/snapshotProbe";
import { defineSecretProviderDriver } from "@t3tools/secret-provider-core/server/driver";
import * as HostProcess from "@t3tools/shared/HostProcess";
import { resolveSpawnCommand } from "@t3tools/shared/shell";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";
import { FetchHttpClient, HttpClient, HttpClientRequest } from "effect/http";
import { ChildProcess } from "effect/process";

import {
  definition,
  parseOpenBaoReference,
  type OpenBaoSecretProviderConfig,
} from "../shared/definition.ts";

const OPENBAO_BINARY = "bao";
// OpenBao never prompts, so an unreachable address should not hold a rebuild for long.
const REQUEST_TIMEOUT = Duration.seconds(10);

const SecretBody = Schema.Struct({
  data: Schema.Struct({ data: Schema.Record(Schema.String, Schema.Unknown) }),
});
const decodeSecretBody = Schema.decodeUnknownOption(SecretBody);
const ErrorBody = Schema.Struct({ errors: Schema.optional(Schema.Array(Schema.String)) });
const decodeErrorBody = Schema.decodeUnknownOption(ErrorBody);

const nonEmpty = (value: string | undefined) =>
  value === undefined || value.length === 0 ? undefined : value;

// `cli` asks `bao print token` with token variables withheld so it reports its own login.
const resolveToken = Effect.fn("OpenBaoDriver.resolveToken")(function* (
  config: OpenBaoSecretProviderConfig,
) {
  switch (config.auth) {
    case "token":
      return nonEmpty(config.token);
    case "env":
      return nonEmpty((yield* HostProcess.Environment).BAO_TOKEN);
    case "cli": {
      const spawnCommand = yield* resolveSpawnCommand(OPENBAO_BINARY, ["print", "token"]);
      const result = yield* spawnAndCollect(
        OPENBAO_BINARY,
        ChildProcess.make(spawnCommand.command, spawnCommand.args, {
          shell: spawnCommand.shell,
          env: {
            BAO_ADDR: config.address,
            ...(config.namespace === undefined ? {} : { BAO_NAMESPACE: config.namespace }),
            // `bao` prefers these over its saved login; Node drops keys whose value is undefined.
            BAO_TOKEN: undefined,
            VAULT_TOKEN: undefined,
          },
          extendEnv: true,
        }),
      );
      if (result.code !== 0) {
        yield* Effect.logWarning("Could not read an OpenBao token from the bao CLI", {
          exitCode: result.code,
          detail: result.stderr.trim(),
        });
        return undefined;
      }
      return nonEmpty(result.stdout.trim());
    }
  }
});

const encodePath = (path: string) => path.split("/").map(encodeURIComponent).join("/");

const readOne = Effect.fn("OpenBaoDriver.readOne")(function* (
  config: OpenBaoSecretProviderConfig,
  token: string,
  reference: string,
) {
  const location = parseOpenBaoReference(reference);
  if (location === undefined) return undefined;
  const client = yield* HttpClient.HttpClient;
  const url = `${config.address.replace(/\/+$/, "")}/v1/${encodePath(config.mount)}/data/${encodePath(location.path)}`;
  const request = HttpClientRequest.get(url).pipe(
    HttpClientRequest.setHeaders({
      "X-Vault-Token": token,
      ...(config.namespace === undefined ? {} : { "X-Vault-Namespace": config.namespace }),
    }),
  );
  // Trace headers would tell the vault which of our spans made the request.
  const response = yield* client
    .execute(request)
    .pipe(
      Effect.provideService(HttpClient.TracerPropagationEnabled, false),
      Effect.timeout(REQUEST_TIMEOUT),
    );
  const body: unknown = yield* response.json.pipe(Effect.orElseSucceed(() => undefined));
  if (response.status < 200 || response.status >= 300) {
    // OpenBao's `errors` array names the failure and never carries a secret.
    const errors = decodeErrorBody(body).pipe(
      Option.flatMap((decoded) => Option.fromNullishOr(decoded.errors)),
      Option.getOrElse((): ReadonlyArray<string> => []),
    );
    yield* Effect.logWarning("Could not read secret from OpenBao", {
      status: response.status,
      errorCount: errors.length,
    });
    return undefined;
  }
  const value = decodeSecretBody(body).pipe(
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
    yield* Effect.logWarning("OpenBao secret has no usable value for the requested field");
    return undefined;
  }
  return secret;
});

export const makeDriver = (
  httpClient: Layer.Layer<HttpClient.HttpClient> = FetchHttpClient.layer,
) =>
  defineSecretProviderDriver({
    kind: definition.kind,
    definition,
    read: Effect.fn("OpenBaoDriver.read")(function* (config, references) {
      const token = yield* resolveToken(config).pipe(
        Effect.catch((error) =>
          Effect.logWarning("Could not run bao", { detail: String(error) }).pipe(
            Effect.as(undefined),
          ),
        ),
      );
      if (token === undefined) {
        yield* Effect.logWarning("OpenBao provider has no token to read with", {
          auth: config.auth,
        });
        return references.map(() => undefined);
      }
      return yield* Effect.forEach(references, (reference) =>
        readOne(config, token, reference).pipe(
          Effect.provide(httpClient),
          Effect.catch((error) =>
            Effect.logWarning("Could not read secret from OpenBao", {
              detail: String(error),
            }).pipe(Effect.as(undefined)),
          ),
        ),
      );
    }),
  });

export const driver = makeDriver();
