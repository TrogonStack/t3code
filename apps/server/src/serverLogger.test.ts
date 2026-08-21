import * as NodePath from "@effect/platform-node/NodePath";
import { assert, describe, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as HttpClient from "effect/unstable/http/HttpClient";
import * as HttpClientResponse from "effect/unstable/http/HttpClientResponse";

import * as ServerConfig from "./config.ts";
import * as OtelEnvironment from "@t3tools/shared/otelEnvironment";
import { ServerLoggerLive } from "./serverLogger.ts";

interface ExportedRequest {
  readonly url: string;
  readonly headers: Readonly<Record<string, string>>;
  readonly body: string;
}

/** Answers every export with a 200 and keeps what was posted for assertions. */
const collectorLayer = (requests: Array<ExportedRequest>) =>
  Layer.succeed(
    HttpClient.HttpClient,
    HttpClient.make((request) =>
      Effect.sync(() => {
        requests.push({
          url: request.url,
          headers: request.headers,
          body:
            request.body._tag === "Uint8Array" ? new TextDecoder().decode(request.body.body) : "",
        });
        return HttpClientResponse.fromWeb(request, new Response(null, { status: 200 }));
      }),
    ),
  );

const configLayer = (overrides: Partial<ServerConfig.ServerConfig["Service"]>) =>
  Layer.effect(
    ServerConfig.ServerConfig,
    Effect.gen(function* () {
      const baseDir = "/tmp/t3-server-logger-test";
      const derivedPaths = yield* ServerConfig.deriveServerPaths(baseDir, undefined);
      return ServerConfig.make({
        logLevel: "Info",
        traceMinLevel: "Info",
        traceTimingEnabled: false,
        traceBatchWindowMs: 200,
        traceMaxBytes: 1024,
        traceMaxFiles: 1,
        otlpTracesUrl: undefined,
        otlpMetricsUrl: undefined,
        otlpLogsUrl: undefined,
        otlpExportIntervalMs: 10_000,
        otlpMetricsExportIntervalMs: 10_000,
        otlpLogsExportIntervalMs: 10_000,
        otlpServiceName: "t3-server",
        otelEnvironment: OtelEnvironment.none,
        cwd: baseDir,
        baseDir,
        ...derivedPaths,
        mode: "web",
        autoBootstrapProjectFromCwd: false,
        logWebSocketEvents: false,
        tailscaleServeEnabled: false,
        tailscaleServePort: 443,
        port: 0,
        host: undefined,
        desktopBootstrapToken: undefined,
        desktopTelemetryFd: undefined,
        desktopTelemetryControlFd: undefined,
        resourceMonitorPath: undefined,
        staticDir: undefined,
        devUrl: undefined,
        devAllowedOrigins: [],
        noBrowser: false,
        startupPresentation: "browser",
        ...overrides,
      });
    }),
  ).pipe(Layer.provide(NodePath.layer));

/**
 * Logs once with the server's own logger set installed, then reports what the
 * collector received. The export is asserted after the layer's scope closes,
 * which is where the exporter flushes whatever the interval did not.
 */
const logThrough = (overrides: Partial<ServerConfig.ServerConfig["Service"]>) =>
  Effect.gen(function* () {
    const requests: Array<ExportedRequest> = [];
    yield* Effect.log("server logger under test").pipe(
      Effect.provide(
        ServerLoggerLive.pipe(
          Layer.provide(configLayer(overrides)),
          Layer.provide(collectorLayer(requests)),
        ),
      ),
    );
    return requests;
  });

describe("ServerLoggerLive", () => {
  it.effect("exports log records to the configured logs endpoint", () =>
    Effect.gen(function* () {
      const requests = yield* logThrough({
        otlpLogsUrl: "https://collector.example.com/v1/logs",
      });

      assert.lengthOf(requests, 1);
      const [request] = requests;
      assert.strictEqual(request?.url, "https://collector.example.com/v1/logs");
      assert.include(request?.body ?? "", "server logger under test");
      assert.include(request?.body ?? "", "t3-server");
      assert.include(request?.body ?? "", "service.runtime");
    }),
  );

  it.effect("stays off the network when no logs endpoint is configured", () =>
    Effect.gen(function* () {
      const requests = yield* logThrough({});

      assert.lengthOf(requests, 0);
    }),
  );

  it.effect("sends the headers and wire format the log signal asked for", () =>
    Effect.gen(function* () {
      const requests = yield* logThrough({
        otlpLogsUrl: "https://collector.example.com/v1/logs",
        otelEnvironment: {
          ...OtelEnvironment.none,
          logs: {
            settings: {
              url: "https://collector.example.com/v1/logs",
              protocol: "http/protobuf",
              headers: { "x-scope": "logs" },
              exportIntervalMs: 1_000,
              maxBatchSize: 512,
              temporality: undefined,
            },
            declined: undefined,
          },
        },
      });

      assert.lengthOf(requests, 1);
      assert.strictEqual(requests[0]?.headers["x-scope"], "logs");
      assert.strictEqual(requests[0]?.headers["content-type"], "application/x-protobuf");
    }),
  );
});
