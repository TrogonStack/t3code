import * as NodeCrypto from "@effect/platform-node/NodeCrypto";
import * as NodeFileSystem from "@effect/platform-node/NodeFileSystem";
import { assert, describe, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Sink from "effect/Sink";
import * as Stream from "effect/Stream";
import { ChildProcessSpawner } from "effect/process";

import { driver } from "./driver.ts";

const encoder = new TextEncoder();
const config = { region: "us-east-1" };

function fakeAws(
  answer: (args: ReadonlyArray<string>) => { stdout?: string; stderr?: string; code?: number },
) {
  const invocations: Array<ReadonlyArray<string>> = [];
  const layer = Layer.mergeAll(
    NodeFileSystem.layer,
    NodeCrypto.layer,
    Layer.succeed(
      ChildProcessSpawner.ChildProcessSpawner,
      ChildProcessSpawner.make((command) =>
        Effect.sync(() => {
          const { args } = command as unknown as { readonly args: ReadonlyArray<string> };
          invocations.push(args);
          const result = answer(args);
          return ChildProcessSpawner.makeHandle({
            pid: ChildProcessSpawner.ProcessId(1),
            exitCode: Effect.succeed(ChildProcessSpawner.ExitCode(result.code ?? 0)),
            isRunning: Effect.succeed(false),
            kill: () => Effect.void,
            unref: Effect.succeed(Effect.void),
            stdin: Sink.drain,
            stdout: Stream.make(encoder.encode(result.stdout ?? "")),
            stderr: Stream.make(encoder.encode(result.stderr ?? "")),
            all: Stream.empty,
            getInputFd: () => Sink.drain,
            getOutputFd: () => Stream.empty,
          });
        }),
      ),
    ),
  );
  return { layer, invocations };
}

const body = (value: { SecretString?: string; SecretBinary?: string }) =>
  JSON.stringify({ ARN: "arn:aws:secretsmanager:us-east-1:000000000000:secret:prod", ...value });

describe("AWS Secrets Manager driver", () => {
  it.effect("reads the whole secret string in the provider's region", () => {
    const aws = fakeAws(() => ({ stdout: body({ SecretString: "sk-aws" }) }));
    return Effect.gen(function* () {
      assert.deepStrictEqual(yield* driver.read(config, ["prod/openai"]), ["sk-aws"]);
      assert.deepStrictEqual(aws.invocations, [
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
    }).pipe(Effect.provide(aws.layer));
  });

  it.effect("passes the named profile", () => {
    const aws = fakeAws(() => ({ stdout: body({ SecretString: "x" }) }));
    return Effect.gen(function* () {
      yield* driver.read({ ...config, profile: "work" }, ["prod/openai"]);
      const args = aws.invocations[0]!;
      assert.deepStrictEqual(args.slice(args.indexOf("--profile"), args.indexOf("--profile") + 2), [
        "--profile",
        "work",
      ]);
    }).pipe(Effect.provide(aws.layer));
  });

  it.effect("reads one key of a JSON secret and reports a missing key unreadable", () => {
    const aws = fakeAws(() => ({
      stdout: body({ SecretString: JSON.stringify({ api_key: "sk", port: 8200, nested: {} }) }),
    }));
    return Effect.gen(function* () {
      assert.deepStrictEqual(
        yield* driver.read(config, [
          "prod/openai#api_key",
          "prod/openai#port",
          "prod/openai#nested",
          "prod/openai#missing",
        ]),
        ["sk", "8200", undefined, undefined],
      );
    }).pipe(Effect.provide(aws.layer));
  });

  it.effect("reads a binary-only secret as undefined", () => {
    const aws = fakeAws(() => ({ stdout: body({ SecretBinary: "AAEC" }) }));
    return Effect.gen(function* () {
      assert.deepStrictEqual(yield* driver.read(config, ["prod/openai"]), [undefined]);
    }).pipe(Effect.provide(aws.layer));
  });

  it.effect("reads a failing aws as undefined", () => {
    const aws = fakeAws(() => ({ code: 254, stderr: "ResourceNotFoundException" }));
    return Effect.gen(function* () {
      assert.deepStrictEqual(yield* driver.read(config, ["prod/openai"]), [undefined]);
    }).pipe(Effect.provide(aws.layer));
  });

  it.effect("refuses a flag-shaped id and an empty key without spawning", () => {
    const aws = fakeAws(() => ({ stdout: body({ SecretString: "x" }) }));
    return Effect.gen(function* () {
      assert.deepStrictEqual(yield* driver.read(config, ["--profile", "prod#"]), [
        undefined,
        undefined,
      ]);
      assert.strictEqual(aws.invocations.length, 0);
    }).pipe(Effect.provide(aws.layer));
  });
});
