import { assert, describe, it } from "@effect/vitest";
import * as ConfigProvider from "effect/ConfigProvider";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import * as OtelEnvironment from "./OtelEnvironment.ts";

const withEnv = (env: Record<string, string>) =>
  Effect.provide(Layer.mergeAll(ConfigProvider.layer(ConfigProvider.fromEnv({ env }))));

describe("OtelEnvironment", () => {
  it.effect("stays off when nothing is configured", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(withEnv({}));
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.strictEqual(resolved.disabled, false);
    }),
  );

  it.effect("appends the signal path to the generic endpoint", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com" }),
      );
      assert.strictEqual(resolved.traces.settings?.url, "https://collector.example.com/v1/traces");
      assert.strictEqual(
        resolved.metrics.settings?.url,
        "https://collector.example.com/v1/metrics",
      );
    }),
  );

  it.effect("does not double the slash on a generic endpoint that has one", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com/" }),
      );
      assert.strictEqual(resolved.traces.settings?.url, "https://collector.example.com/v1/traces");
    }),
  );

  it.effect("takes a signal endpoint exactly as written", () =>
    Effect.gen(function* () {
      // The per-signal variable is a whole URL. Appending to it would send
      // traces to a path the collector does not serve.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://generic.example.com",
          OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: "https://traces.example.com/ingest",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.url, "https://traces.example.com/ingest");
      assert.strictEqual(resolved.metrics.settings?.url, "https://generic.example.com/v1/metrics");
    }),
  );

  it.effect("exports without OTEL_TRACES_EXPORTER, because otlp is its default", () =>
    Effect.gen(function* () {
      // A machine that sets OTEL_METRICS_EXPORTER and leaves the traces one
      // alone still wants traces; the spec default is otlp, not none.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_METRICS_EXPORTER: "otlp",
        }),
      );
      assert.isDefined(resolved.traces.settings);
    }),
  );

  it.effect("honors a signal turned off by name", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_TRACES_EXPORTER: "none",
        }),
      );
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.isDefined(resolved.metrics.settings);
    }),
  );

  it.effect("finds otlp in a list of exporters", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_TRACES_EXPORTER: "console, otlp",
        }),
      );
      assert.isDefined(resolved.traces.settings);
    }),
  );

  it.effect("exports nothing when the SDK is disabled", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_SDK_DISABLED: "true",
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
        }),
      );
      assert.strictEqual(resolved.disabled, true);
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.strictEqual(resolved.metrics.settings, undefined);
    }),
  );

  it.effect("carries the headers a collector needs to accept the request", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_HEADERS: "api-key=abc123,x-tenant=acme",
          OTEL_EXPORTER_OTLP_TRACES_HEADERS: "api-key=traces-only",
        }),
      );
      // The per-signal header set replaces the generic one rather than
      // merging with it, which is what the spec says and what a collector
      // with two different keys depends on.
      assert.deepStrictEqual(resolved.traces.settings?.headers, { "api-key": "traces-only" });
      assert.deepStrictEqual(resolved.metrics.settings?.headers, {
        "api-key": "abc123",
        "x-tenant": "acme",
      });
    }),
  );

  it.effect("reads the service identity and the leftover resource attributes", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_RESOURCE_ATTRIBUTES: "org.name=Example,service.name=from-attributes,deployment=prod",
          OTEL_SERVICE_VERSION: "1.2.3",
        }),
      );
      assert.strictEqual(resolved.resource.serviceName, "from-attributes");
      assert.strictEqual(resolved.resource.serviceVersion, "1.2.3");
      // service.name and service.version become the named fields, so leaving
      // them in the attribute bag too would send each one twice.
      assert.deepStrictEqual(resolved.resource.attributes, {
        "org.name": "Example",
        deployment: "prod",
      });
    }),
  );

  it.effect("lets OTEL_SERVICE_NAME win over the resource attribute", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_SERVICE_NAME: "explicit",
          OTEL_RESOURCE_ATTRIBUTES: "service.name=from-attributes",
        }),
      );
      assert.strictEqual(resolved.resource.serviceName, "explicit");
    }),
  );

  it.effect("declines grpc instead of posting a body it cannot frame", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_PROTOCOL: "grpc",
        }),
      );
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.include(resolved.traces.declined ?? "", "grpc");
    }),
  );

  it.effect("declines only the signal that asked for grpc", () =>
    Effect.gen(function* () {
      // A metric endpoint that speaks gRPC says nothing about where traces go,
      // and turning traces off over it loses telemetry nobody asked to lose.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_METRICS_PROTOCOL: "grpc",
        }),
      );
      assert.isDefined(resolved.traces.settings);
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.include(resolved.metrics.declined ?? "", "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL");
    }),
  );

  it.effect("defaults each signal to the specification's wire format", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com" }),
      );
      assert.strictEqual(resolved.traces.settings?.protocol, "http/protobuf");
      assert.strictEqual(resolved.metrics.settings?.protocol, "http/protobuf");
    }),
  );

  it.effect("keeps the wire format on the signal that named an endpoint", () =>
    Effect.gen(function* () {
      // A protocol with no endpoint of its own describes nothing, so it must
      // not reach an export configured by some other name.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_EXPORTER_OTLP_PROTOCOL: "http/protobuf" }),
      );
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.strictEqual(resolved.metrics.settings, undefined);
    }),
  );

  it.effect("takes the batch and timeout knobs the exporter can act on", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_BSP_SCHEDULE_DELAY: "2500",
          OTEL_BSP_MAX_EXPORT_BATCH_SIZE: "128",
          OTEL_EXPORTER_OTLP_TIMEOUT: "7000",
          OTEL_METRIC_EXPORT_INTERVAL: "15000",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 2500);
      assert.strictEqual(resolved.traces.settings?.maxBatchSize, 128);
      assert.strictEqual(resolved.traces.settings?.shutdownTimeoutMs, 7000);
      assert.strictEqual(resolved.metrics.settings?.exportIntervalMs, 15000);
    }),
  );

  it.effect("falls back to the specification's own batching defaults", () =>
    Effect.gen(function* () {
      // Once this route is the one configuring the exporter, the numbers that
      // apply are the specification's, not the ones T3 Code picked for itself.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com" }),
      );
      assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 5000);
      assert.strictEqual(resolved.traces.settings?.maxBatchSize, 512);
      assert.strictEqual(resolved.metrics.settings?.exportIntervalMs, 60000);
    }),
  );

  it.effect("lets the metric signal name its own timeout and temporality", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_METRIC_EXPORT_TIMEOUT: "9000",
          OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: "delta",
        }),
      );
      assert.strictEqual(resolved.metrics.settings?.shutdownTimeoutMs, 9000);
      assert.strictEqual(resolved.metrics.settings?.temporality, "delta");
    }),
  );

  it.effect("decodes a header the way the specification encodes it", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_HEADERS: "Authorization=Bearer%20abc123, x-scope=team%2Fplatform",
        }),
      );
      assert.deepStrictEqual(resolved.traces.settings?.headers, {
        Authorization: "Bearer abc123",
        "x-scope": "team/platform",
      });
    }),
  );

  it.effect("keeps a credential that contains its own separator", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_HEADERS: "Authorization=Basic YWJjOmRlZg==",
        }),
      );
      assert.deepStrictEqual(resolved.traces.settings?.headers, {
        Authorization: "Basic YWJjOmRlZg==",
      });
    }),
  );

  it.effect("decodes resource attributes too", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_RESOURCE_ATTRIBUTES: "team=platform%20eng, deployment.environment=prod" }),
      );
      assert.deepStrictEqual(resolved.resource.attributes, {
        team: "platform eng",
        "deployment.environment": "prod",
      });
    }),
  );

  it.effect("discards a pair list that is not valid percent encoding", () =>
    Effect.gen(function* () {
      // Half a header set is worse than none: the collector answers a partial
      // credential with the same 401 it gives a wrong one, and nothing says
      // the variable was the problem.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_HEADERS: "x-token=100%zz,x-other=100%25",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.headers, undefined);
      assert.isTrue(
        resolved.warnings.some((warning) => warning.includes("OTEL_EXPORTER_OTLP_HEADERS")),
      );
    }),
  );

  it.effect("discards resource attributes that do not decode", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_RESOURCE_ATTRIBUTES: "team=100%zz,deployment=prod" }),
      );
      assert.deepStrictEqual(resolved.resource.attributes, {});
      assert.isTrue(
        resolved.warnings.some((warning) => warning.includes("OTEL_RESOURCE_ATTRIBUTES")),
      );
    }),
  );

  it.effect("appends the signal path after a base that already has one", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com/otel" }),
      );
      assert.strictEqual(
        resolved.traces.settings?.url,
        "https://collector.example.com/otel/v1/traces",
      );
    }),
  );

  it.effect("warns about a temporality this exporter cannot produce", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: "lowmemory",
        }),
      );
      assert.strictEqual(resolved.metrics.settings?.temporality, undefined);
      assert.isDefined(resolved.metrics.settings);
      assert.isTrue(resolved.warnings.some((warning) => warning.includes("lowmemory")));
    }),
  );

  it.effect("warns about a misspelled protocol and keeps exporting", () =>
    Effect.gen(function* () {
      // The specification is explicit here: a value the implementation does
      // not recognize gets a warning and is ignored. Switching export off over
      // a typo loses the telemetry the typo was not about.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_PROTOCOL: "htp/json",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.protocol, "http/protobuf");
      assert.strictEqual(resolved.traces.declined, undefined);
      assert.isTrue(resolved.warnings.some((warning) => warning.includes("htp/json")));
    }),
  );

  it.effect("lets the two signals use different wire formats", () =>
    Effect.gen(function* () {
      // Each signal builds its own serializer, so the metric protocol is
      // honored on its own rather than losing to the trace one.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_PROTOCOL: "http/json",
          OTEL_EXPORTER_OTLP_METRICS_PROTOCOL: "http/protobuf",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.protocol, "http/json");
      assert.strictEqual(resolved.metrics.settings?.protocol, "http/protobuf");
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );

  it.effect("reads the metric protocol when it is the only one named", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_METRICS_PROTOCOL: "http/json",
        }),
      );
      assert.strictEqual(resolved.metrics.settings?.protocol, "http/json");
      assert.strictEqual(resolved.traces.settings?.protocol, "http/protobuf");
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );
});
