import { assert, describe, it } from "@effect/vitest";
import * as OtelEnvironment from "@t3tools/shared/otelEnvironment";
import * as ConfigProvider from "effect/ConfigProvider";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import {
  DEFAULT_DESKTOP_EXPORT_INTERVAL_MS,
  type DesktopNamedOtlpEndpoints,
  resolveDesktopOtlpExport,
} from "./DesktopOtlpExport.ts";

const noNamedEndpoints: DesktopNamedOtlpEndpoints = {
  traces: undefined,
  metrics: undefined,
  logs: undefined,
};

const resolve = (
  env: Record<string, string>,
  overrides: {
    readonly named?: Partial<DesktopNamedOtlpEndpoints>;
    readonly namedExportIntervalMs?: number;
  } = {},
) =>
  OtelEnvironment.load.pipe(
    Effect.provide(Layer.mergeAll(ConfigProvider.layer(ConfigProvider.fromEnv({ env })))),
    Effect.map((otel) =>
      resolveDesktopOtlpExport({
        otel,
        named: { ...noNamedEndpoints, ...overrides.named },
        namedExportIntervalMs: overrides.namedExportIntervalMs,
        serviceName: "t3-desktop",
        runtimeAttributes: { "service.runtime": "desktop", "service.mode": "development" },
      }),
    ),
  );

describe("resolveDesktopOtlpExport", () => {
  it.effect("exports nothing when neither T3 Code nor OpenTelemetry named an endpoint", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve({});
      assert.strictEqual(resolved.traces.url, undefined);
      assert.strictEqual(resolved.metrics.url, undefined);
      assert.strictEqual(resolved.logs.url, undefined);
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );

  it.effect("picks up all three signals from the generic OpenTelemetry endpoint", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve({
        OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
      });
      assert.strictEqual(resolved.traces.url, "https://collector.example.com/v1/traces");
      assert.strictEqual(resolved.metrics.url, "https://collector.example.com/v1/metrics");
      assert.strictEqual(resolved.logs.url, "https://collector.example.com/v1/logs");
      assert.strictEqual(resolved.traces.protocol, "http/protobuf");
    }),
  );

  it.effect("lets a named endpoint take the whole signal, not only its url", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve(
        {
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_PROTOCOL: "http/protobuf",
          OTEL_EXPORTER_OTLP_HEADERS: "authorization=Bearer%20token",
        },
        { named: { traces: "http://127.0.0.1:4318/v1/traces" } },
      );

      assert.strictEqual(resolved.traces.url, "http://127.0.0.1:4318/v1/traces");
      assert.strictEqual(resolved.traces.protocol, "http/json");
      assert.strictEqual(resolved.traces.headers, undefined);
      assert.strictEqual(resolved.traces.exportIntervalMs, DEFAULT_DESKTOP_EXPORT_INTERVAL_MS);

      assert.strictEqual(resolved.metrics.url, "https://collector.example.com/v1/metrics");
      assert.strictEqual(resolved.metrics.protocol, "http/protobuf");
      assert.deepStrictEqual(resolved.metrics.headers, { authorization: "Bearer token" });
    }),
  );

  it.effect("stops every export when the OpenTelemetry SDK is disabled", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve(
        {
          OTEL_SDK_DISABLED: "true",
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
        },
        { named: { traces: "http://127.0.0.1:4318/v1/traces" } },
      );
      assert.strictEqual(resolved.traces.url, undefined);
      assert.strictEqual(resolved.metrics.url, undefined);
      assert.strictEqual(resolved.logs.url, undefined);
      assert.include(resolved.warnings.join("\n"), "OTEL_SDK_DISABLED");
    }),
  );

  it.effect("declines only the signal that asked for a protocol T3 Code cannot speak", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve({
        OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
        OTEL_EXPORTER_OTLP_TRACES_PROTOCOL: "grpc",
      });
      assert.strictEqual(resolved.traces.url, undefined);
      assert.strictEqual(resolved.metrics.url, "https://collector.example.com/v1/metrics");
      assert.strictEqual(resolved.logs.url, "https://collector.example.com/v1/logs");
      assert.lengthOf(resolved.warnings, 1);
      assert.include(resolved.warnings[0] ?? "", "grpc");
    }),
  );

  it.effect("keeps exporting a signal whose grpc endpoint another variable overrode", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve(
        {
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_TRACES_PROTOCOL: "grpc",
        },
        { named: { traces: "http://127.0.0.1:4318/v1/traces" } },
      );
      assert.strictEqual(resolved.traces.url, "http://127.0.0.1:4318/v1/traces");
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );

  it.effect("honors one T3 Code interval across every signal", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve(
        {
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_METRIC_EXPORT_INTERVAL: "30000",
        },
        { namedExportIntervalMs: 2500 },
      );
      assert.strictEqual(resolved.traces.exportIntervalMs, 2500);
      assert.strictEqual(resolved.metrics.exportIntervalMs, 2500);
      assert.strictEqual(resolved.logs.exportIntervalMs, 2500);
    }),
  );

  it.effect("falls back to the interval the specification defines for each signal", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve({
        OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
      });
      assert.strictEqual(resolved.traces.exportIntervalMs, 5_000);
      assert.strictEqual(resolved.metrics.exportIntervalMs, 60_000);
      assert.strictEqual(resolved.logs.exportIntervalMs, 1_000);
    }),
  );

  it.effect("cannot be made to claim it is the server process", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve({
        OTEL_SERVICE_VERSION: "1.2.3",
        OTEL_RESOURCE_ATTRIBUTES: "deployment.environment=lab,service.runtime=t3-server",
      });
      assert.strictEqual(resolved.resource.serviceName, "t3-desktop");
      assert.strictEqual(resolved.resource.serviceVersion, "1.2.3");
      assert.strictEqual(resolved.resource.attributes["deployment.environment"], "lab");
      assert.strictEqual(resolved.resource.attributes["service.runtime"], "desktop");
    }),
  );

  it.effect("cannot be renamed by the environment", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve({
        OTEL_SERVICE_NAME: "some-other-app",
        OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
      });
      assert.strictEqual(resolved.resource.serviceName, "t3-desktop");
      assert.lengthOf(resolved.warnings, 1);
      assert.include(resolved.warnings[0] ?? "", "OTEL_SERVICE_NAME was ignored");
    }),
  );

  it.effect("cannot be renamed through the resource attributes either", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve({
        OTEL_RESOURCE_ATTRIBUTES: "service.name=some-other-app,host.name=lab-01",
      });
      assert.strictEqual(resolved.resource.serviceName, "t3-desktop");
      assert.strictEqual(resolved.resource.attributes["service.name"], undefined);
      assert.strictEqual(resolved.resource.attributes["host.name"], "lab-01");
      assert.include(resolved.warnings[0] ?? "", "service.name was ignored");
    }),
  );

  it.effect("names itself even when the environment says nothing", () =>
    Effect.gen(function* () {
      const resolved = yield* resolve({});
      assert.strictEqual(resolved.resource.serviceName, "t3-desktop");
      assert.strictEqual(resolved.resource.serviceVersion, undefined);
    }),
  );
});
