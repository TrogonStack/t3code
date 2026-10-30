import { spawnAndCollect } from "@t3tools/provider-core/server/snapshotProbe";
import { defineSecretProviderDriver } from "@t3tools/secret-provider-core/server/driver";
import { resolveSpawnCommand } from "@t3tools/shared/shell";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";
import { ChildProcess } from "effect/process";

import {
  definition,
  parseAwsReference,
  type AwsSecretsManagerSecretProviderConfig,
} from "../shared/definition.ts";

const AWS_BINARY = "aws";
const READ_TIMEOUT = Duration.seconds(45);

const SecretValueJson = Schema.fromJsonString(
  Schema.Struct({
    SecretString: Schema.optional(Schema.String),
    SecretBinary: Schema.optional(Schema.String),
  }),
);
const decodeSecretValue = Schema.decodeUnknownOption(SecretValueJson);
const decodeJson = Schema.decodeUnknownOption(Schema.fromJsonString(Schema.Unknown));

const nonEmpty = (value: string) => (value.length === 0 ? undefined : value);

const readOne = Effect.fn("AwsSecretsManagerDriver.readOne")(function* (
  config: AwsSecretsManagerSecretProviderConfig,
  reference: string,
) {
  const location = parseAwsReference(reference);
  if (location === undefined) return undefined;
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
    // `aws` reports credential and not-found failures on stderr and never echoes the secret.
    yield* Effect.logWarning("Could not read secret from AWS Secrets Manager", {
      exitCode: result.code,
      detail: result.stderr.trim(),
    });
    return undefined;
  }
  const body = Option.getOrUndefined(decodeSecretValue(result.stdout));
  if (body?.SecretString === undefined) {
    yield* Effect.logWarning("AWS Secrets Manager returned no secret string");
    return undefined;
  }
  if (location.key === undefined) return nonEmpty(body.SecretString);
  const parsed = Option.getOrUndefined(decodeJson(body.SecretString));
  const value =
    typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
      ? Reflect.get(parsed, location.key)
      : undefined;
  if (typeof value === "string") return nonEmpty(value);
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  yield* Effect.logWarning("AWS Secrets Manager secret has no scalar value for the key");
  return undefined;
});

export const driver = defineSecretProviderDriver({
  kind: definition.kind,
  definition,
  read: Effect.fn("AwsSecretsManagerDriver.read")(function* (config, references) {
    return yield* Effect.forEach(references, (reference) =>
      readOne(config, reference).pipe(
        Effect.timeoutOption(READ_TIMEOUT),
        Effect.map(Option.getOrUndefined),
        Effect.catch((error) =>
          Effect.logWarning("Could not run aws", { error: String(error) }).pipe(
            Effect.as(undefined),
          ),
        ),
      ),
    );
  }),
});
