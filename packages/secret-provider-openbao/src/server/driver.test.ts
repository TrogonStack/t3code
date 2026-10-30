import * as NodeCrypto from "@effect/platform-node/NodeCrypto";
import * as NodeFileSystem from "@effect/platform-node/NodeFileSystem";
import { assert, describe, it } from "@effect/vitest";
import * as HostProcess from "@t3tools/shared/HostProcess";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Sink from "effect/Sink";
import * as Stream from "effect/Stream";
import { HttpClient, HttpClientResponse } from "effect/http";
import { ChildProcessSpawner } from "effect/process";

import { makeDriver } from "./driver.ts";

const encoder = new TextEncoder();
const ADDRESS = "https://bao.example";

function fakeBao(result: { stdout?: string; stderr?: string; code?: number } = {}) {
  const commands: Array<string> = [];
  const layer = Layer.mergeAll(
    NodeFileSystem.layer,
    NodeCrypto.layer,
    Layer.succeed(
      ChildProcessSpawner.ChildProcessSpawner,
      ChildProcessSpawner.make((command) =>
        Effect.sync(() => {
          commands.push((command as unknown as { command: string }).command);
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
  return { layer, commands };
}

function fakeHttp(status: number, body: unknown) {
  const requests: Array<{ url: string; headers: Record<string, string> }> = [];
  const layer = Layer.succeed(
    HttpClient.HttpClient,
    HttpClient.make((request) =>
      Effect.sync(() => {
        requests.push({ url: request.url, headers: { ...request.headers } });
        return HttpClientResponse.fromWeb(request, new Response(JSON.stringify(body), { status }));
      }),
    ),
  );
  return { layer, requests };
}

const kv = { data: { data: { api_key: "sk-api", port: 8200, nested: {} } } };
const base = { address: ADDRESS, mount: "kv", auth: "token" as const, token: "s.configured" };

describe("OpenBao driver", () => {
  it.effect("reads the KV v2 field with the stored token and namespace", () => {
    const http = fakeHttp(200, kv);
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      assert.deepStrictEqual(
        yield* driver.read({ ...base, namespace: "team" }, [
          "agents/codex#api_key",
          "agents/codex#port",
        ]),
        ["sk-api", "8200"],
      );
      assert.deepStrictEqual(http.requests[0], {
        url: `${ADDRESS}/v1/kv/data/agents/codex`,
        headers: { "x-vault-token": "s.configured", "x-vault-namespace": "team" },
      });
    }).pipe(Effect.provide(fakeBao().layer));
  });

  it.effect("joins a trailing slash address and encodes unsafe path characters", () => {
    const http = fakeHttp(200, kv);
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      yield* driver.read({ ...base, address: `${ADDRESS}/` }, ["my agents/co?dex#api_key"]);
      assert.strictEqual(http.requests[0]?.url, `${ADDRESS}/v1/kv/data/my%20agents/co%3Fdex`);
    }).pipe(Effect.provide(fakeBao().layer));
  });

  it.effect("reads nothing without a request when the stored token is missing", () => {
    const http = fakeHttp(200, kv);
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      assert.deepStrictEqual(
        yield* driver.read({ address: ADDRESS, mount: "kv", auth: "token" }, ["a#b"]),
        [undefined],
      );
      assert.strictEqual(http.requests.length, 0);
    }).pipe(Effect.provide(fakeBao().layer));
  });

  it.effect("uses BAO_TOKEN from the server environment when auth is env", () => {
    const http = fakeHttp(200, kv);
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      assert.deepStrictEqual(
        yield* driver.read({ address: ADDRESS, mount: "kv", auth: "env" }, [
          "agents/codex#api_key",
        ]),
        ["sk-api"],
      );
      assert.strictEqual(http.requests[0]?.headers["x-vault-token"], "s.env");
    }).pipe(
      Effect.provideService(HostProcess.Environment, { BAO_TOKEN: "s.env" }),
      Effect.provide(fakeBao().layer),
    );
  });

  it.effect("reads nothing when auth is env and BAO_TOKEN is unset", () => {
    const http = fakeHttp(200, kv);
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      assert.deepStrictEqual(
        yield* driver.read({ address: ADDRESS, mount: "kv", auth: "env" }, ["a#b"]),
        [undefined],
      );
      assert.strictEqual(http.requests.length, 0);
    }).pipe(Effect.provideService(HostProcess.Environment, {}), Effect.provide(fakeBao().layer));
  });

  it.effect("uses the output of `bao print token` when auth is cli", () => {
    const http = fakeHttp(200, kv);
    const bao = fakeBao({ stdout: "s.from-cli\n" });
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      assert.deepStrictEqual(
        yield* driver.read({ address: ADDRESS, mount: "kv", auth: "cli" }, [
          "agents/codex#api_key",
        ]),
        ["sk-api"],
      );
      assert.deepStrictEqual(bao.commands, ["bao"]);
      assert.strictEqual(http.requests[0]?.headers["x-vault-token"], "s.from-cli");
    }).pipe(Effect.provide(bao.layer));
  });

  it.effect("reads nothing when `bao print token` fails", () => {
    const http = fakeHttp(200, kv);
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      assert.deepStrictEqual(
        yield* driver.read({ address: ADDRESS, mount: "kv", auth: "cli" }, ["a#b"]),
        [undefined],
      );
      assert.strictEqual(http.requests.length, 0);
    }).pipe(Effect.provide(fakeBao({ code: 2, stderr: "no token" }).layer));
  });

  it.effect("reads a 403 as undefined", () => {
    const http = fakeHttp(403, { errors: ["permission denied"] });
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      assert.deepStrictEqual(yield* driver.read(base, ["agents/codex#api_key"]), [undefined]);
    }).pipe(Effect.provide(fakeBao().layer));
  });

  it.effect("reads a missing or non-scalar field as undefined", () => {
    const http = fakeHttp(200, kv);
    const driver = makeDriver(http.layer);
    return Effect.gen(function* () {
      assert.deepStrictEqual(
        yield* driver.read(base, ["agents/codex#missing", "agents/codex#nested"]),
        [undefined, undefined],
      );
    }).pipe(Effect.provide(fakeBao().layer));
  });
});
