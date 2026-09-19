import { assert, describe, it } from "@effect/vitest";
import * as ConfigProvider from "effect/ConfigProvider";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import * as OtelEnvironment from "./otelEnvironment.ts";

const withEnv = (env: Record<string, string>) =>
  Effect.provide(Layer.mergeAll(ConfigProvider.layer(ConfigProvider.fromEnv({ env }))));

describe("OtelEnvironment", () => {
  it.effect("stays off when nothing is configured", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(withEnv({}));
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.strictEqual(resolved.logs.settings, undefined);
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
      assert.strictEqual(resolved.logs.settings?.url, "https://collector.example.com/v1/logs");
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
      assert.isTrue(resolved.warnings.some((warning) => warning.includes("console, otlp")));
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
      // Someone who inherited this from a shell profile has somewhere to go.
      assert.include(resolved.warnings.join("\n"), "T3CODE_OTEL_SDK_DISABLED=false");
    }),
  );

  it.effect("lets T3 Code's own name answer before the standard one", () =>
    Effect.gen(function* () {
      const off = yield* OtelEnvironment.load.pipe(
        withEnv({
          T3CODE_OTEL_SDK_DISABLED: "true",
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
        }),
      );
      assert.isTrue(off.disabled);
      assert.strictEqual(off.traces.settings, undefined);
      assert.deepStrictEqual(off.warnings, [
        "T3CODE_OTEL_SDK_DISABLED is set, so no telemetry is exported, whatever configured it",
      ]);

      // The point of reading ours first: a machine that disables every other
      // SDK in its shell profile can still ask for T3 Code's telemetry.
      const on = yield* OtelEnvironment.load.pipe(
        withEnv({
          T3CODE_OTEL_SDK_DISABLED: "false",
          OTEL_SDK_DISABLED: "true",
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
        }),
      );
      assert.isFalse(on.disabled);
      assert.isDefined(on.traces.settings);
      assert.deepStrictEqual(on.warnings, []);
    }),
  );

  it.effect("reads T3 Code's own name the way T3 Code reads a boolean", () =>
    Effect.gen(function* () {
      // Ours to define, so it takes the affirmatives people type. The
      // specification's single-value rule stays with the OTEL_* name.
      const numeric = yield* OtelEnvironment.load.pipe(withEnv({ T3CODE_OTEL_SDK_DISABLED: "1" }));
      assert.isTrue(numeric.disabled);

      // A value that answers nothing leaves the source under it to answer.
      const nonsense = yield* OtelEnvironment.load.pipe(
        withEnv({
          T3CODE_OTEL_SDK_DISABLED: "maybe",
          OTEL_SDK_DISABLED: "true",
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
        }),
      );
      assert.isTrue(nonsense.disabled);
      assert.include(nonsense.warnings.join("\n"), "T3CODE_OTEL_SDK_DISABLED=maybe");
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

  it.effect("reads the service version and the leftover resource attributes", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_RESOURCE_ATTRIBUTES: "org.name=Example,deployment=prod",
          OTEL_SERVICE_VERSION: "1.2.3",
        }),
      );
      assert.strictEqual(resolved.resource.serviceVersion, "1.2.3");
      // service.version becomes a named field, so leaving it in the attribute
      // bag too would send it twice.
      assert.deepStrictEqual(resolved.resource.attributes, {
        "org.name": "Example",
        deployment: "prod",
      });
    }),
  );

  it.effect("refuses to rename the service, and says so", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_SERVICE_NAME: "some-other-app" }),
      );
      assert.deepStrictEqual(resolved.resource.attributes, {});
      assert.lengthOf(resolved.warnings, 1);
      assert.include(resolved.warnings[0] ?? "", "OTEL_SERVICE_NAME was ignored");
    }),
  );

  it.effect("drops a service.name hidden in the resource attributes", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_RESOURCE_ATTRIBUTES: "service.name=some-other-app,host.name=lab-01" }),
      );
      // Dropped rather than passed through, or the exporter would receive a
      // second service.name beside the one the process chose.
      assert.deepStrictEqual(resolved.resource.attributes, { "host.name": "lab-01" });
      assert.include(resolved.warnings[0] ?? "", "service.name was ignored");
    }),
  );

  it.effect("names OTEL_SERVICE_NAME rather than the attribute when both are set", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_SERVICE_NAME: "explicit",
          OTEL_RESOURCE_ATTRIBUTES: "service.name=from-attributes",
        }),
      );
      assert.lengthOf(resolved.warnings, 1);
      assert.include(resolved.warnings[0] ?? "", "OTEL_SERVICE_NAME was ignored");
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
      assert.strictEqual(resolved.logs.settings?.protocol, "http/protobuf");
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

  it.effect("takes the batch knobs the exporter can act on", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_BSP_SCHEDULE_DELAY: "2500",
          OTEL_BSP_MAX_EXPORT_BATCH_SIZE: "128",
          OTEL_METRIC_EXPORT_INTERVAL: "15000",
          OTEL_BLRP_SCHEDULE_DELAY: "3500",
          OTEL_BLRP_MAX_EXPORT_BATCH_SIZE: "64",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 2500);
      assert.strictEqual(resolved.traces.settings?.maxBatchSize, 128);
      assert.strictEqual(resolved.metrics.settings?.exportIntervalMs, 15000);
      assert.strictEqual(resolved.logs.settings?.exportIntervalMs, 3500);
      assert.strictEqual(resolved.logs.settings?.maxBatchSize, 64);
    }),
  );

  it.effect("leaves the request timeouts alone rather than spending them on shutdown", () =>
    Effect.gen(function* () {
      // These name a per-request deadline and the exporter has no such knob.
      // Bounding the final flush with them instead would hold a restart open
      // for as long as the collector was allowed to be slow.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_TIMEOUT: "600000",
          OTEL_EXPORTER_OTLP_TRACES_TIMEOUT: "600000",
          OTEL_METRIC_EXPORT_TIMEOUT: "600000",
        }),
      );
      assert.deepStrictEqual(Object.keys(resolved.traces.settings ?? {}).sort(), [
        "exportIntervalMs",
        "headers",
        "maxBatchSize",
        "protocol",
        "temporality",
        "url",
      ]);
      assert.deepStrictEqual(resolved.warnings, []);
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
      assert.strictEqual(resolved.logs.settings?.exportIntervalMs, 1000);
      assert.strictEqual(resolved.logs.settings?.maxBatchSize, 512);
    }),
  );

  it.effect("lets the metric signal name its own aggregation", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: "delta",
        }),
      );
      assert.strictEqual(resolved.metrics.settings?.temporality, "delta");
      assert.strictEqual(resolved.traces.settings?.temporality, undefined);
      assert.strictEqual(resolved.logs.settings?.temporality, undefined);
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

  it.effect("resolves lowmemory to the aggregation it asks for on these metrics", () =>
    Effect.gen(function* () {
      // Falling back to the default here would invert the request rather than
      // decline it, and invert it toward the value a delta-only receiver drops
      // without an error, so the timers would vanish and the counters would not.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: "lowmemory",
        }),
      );
      assert.strictEqual(resolved.metrics.settings?.temporality, "delta");
      assert.isTrue(resolved.warnings.some((warning) => warning.includes("lowmemory")));
    }),
  );

  it.effect("ignores a temporality that is not a preference at all", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: "hourly",
        }),
      );
      assert.isDefined(resolved.metrics.settings);
      assert.strictEqual(resolved.metrics.settings?.temporality, undefined);
      assert.isTrue(resolved.warnings.some((warning) => warning.includes("hourly")));
    }),
  );

  it.effect("asks for no aggregation when nothing names one", () =>
    Effect.gen(function* () {
      // Left unset on purpose. The exporter applies
      // `DEFAULT_METRICS_TEMPORALITY`, and a value here would claim the
      // operator chose it.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com" }),
      );
      assert.isDefined(resolved.metrics.settings);
      assert.strictEqual(resolved.metrics.settings?.temporality, undefined);
      assert.deepStrictEqual(resolved.warnings, []);
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

  it.effect("keeps exporting when a number is not a number", () =>
    Effect.gen(function* () {
      // A typo on one knob must not take the rest of the telemetry with it.
      // Before this, the read failed outright and nothing was exported.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_BSP_SCHEDULE_DELAY: "abc",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 5000);
      assert.strictEqual(
        resolved.metrics.settings?.url,
        "https://collector.example.com/v1/metrics",
      );
      assert.isTrue(
        resolved.warnings.some((warning) => warning.includes("OTEL_BSP_SCHEDULE_DELAY")),
      );
    }),
  );

  it.effect("reads a boolean the way the specification defines one", () =>
    Effect.gen(function* () {
      // Case insensitive `true` and nothing else. `yes` is affirmative in
      // other config systems and false here, which the specification is
      // explicit about.
      const upper = yield* OtelEnvironment.load.pipe(withEnv({ OTEL_SDK_DISABLED: "True" }));
      assert.isTrue(upper.disabled);

      const affirmative = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_SDK_DISABLED: "yes",
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
        }),
      );
      assert.isFalse(affirmative.disabled);
      assert.isDefined(affirmative.traces.settings);
    }),
  );

  it.effect("treats an empty value as an unset one", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_SERVICE_NAME: "",
          OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: "",
        }),
      );
      // An empty rename is not a rename, so it is not worth a warning either.
      assert.deepStrictEqual(resolved.warnings, []);
      assert.strictEqual(resolved.traces.settings?.url, "https://collector.example.com/v1/traces");
    }),
  );

  it.effect("falls back to the generic headers when the signal's own list is junk", () =>
    Effect.gen(function* () {
      // A list with no pair in it is malformed, not a request for no headers.
      // Reading it as an answer would shadow the generic variable and send an
      // unauthenticated stream to a collector that was told how to authorize.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_TRACES_HEADERS: "junk",
          OTEL_EXPORTER_OTLP_HEADERS: "Authorization=Bearer%20abc123",
        }),
      );
      assert.deepStrictEqual(resolved.traces.settings?.headers, {
        Authorization: "Bearer abc123",
      });
      assert.isTrue(
        resolved.warnings.some((warning) => warning.includes("OTEL_EXPORTER_OTLP_TRACES_HEADERS")),
      );
    }),
  );

  it.effect("names a shared variable once even though both signals read it", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_HEADERS: "x-token=100%zz",
        }),
      );
      assert.strictEqual(
        resolved.warnings.filter((warning) => warning.includes("OTEL_EXPORTER_OTLP_HEADERS"))
          .length,
        1,
      );
    }),
  );

  it.effect("does not blame gRPC for a signal that was never going to export", () =>
    Effect.gen(function* () {
      // Nothing named an endpoint, so the protocol is beside the point and
      // reporting it would send someone looking for a collector problem.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_EXPORTER_OTLP_PROTOCOL: "grpc" }),
      );
      assert.strictEqual(resolved.traces.declined, undefined);
      assert.strictEqual(resolved.metrics.declined, undefined);
    }),
  );

  it.effect("stays quiet about the protocol once the SDK is off", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_SDK_DISABLED: "true",
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_PROTOCOL: "grpc",
        }),
      );
      assert.isTrue(resolved.disabled);
      assert.strictEqual(resolved.traces.declined, undefined);
      assert.strictEqual(resolved.metrics.declined, undefined);
    }),
  );

  it.effect("does not carry a padded variable into the URL it builds", () =>
    Effect.gen(function* () {
      // A shell profile that lined up its exports did not mean the padding to
      // become part of the endpoint, and the appended signal path would put it
      // in the middle of the URL where nothing would report it.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "  https://collector.example.com/  ",
          OTEL_SERVICE_VERSION: "  1.2.3  ",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.url, "https://collector.example.com/v1/traces");
      assert.strictEqual(resolved.resource.serviceVersion, "1.2.3");
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

  it.effect("takes a log endpoint exactly as written", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://generic.example.com",
          OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: "https://logs.example.com/ingest",
        }),
      );
      assert.strictEqual(resolved.logs.settings?.url, "https://logs.example.com/ingest");
      assert.strictEqual(resolved.traces.settings?.url, "https://generic.example.com/v1/traces");
    }),
  );

  it.effect("honors the log signal turned off by name", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_LOGS_EXPORTER: "none",
        }),
      );
      assert.strictEqual(resolved.logs.settings, undefined);
      assert.isDefined(resolved.traces.settings);
      assert.isDefined(resolved.metrics.settings);
    }),
  );

  it.effect("lets the log signal name its own wire format and headers", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_PROTOCOL: "http/protobuf",
          OTEL_EXPORTER_OTLP_LOGS_PROTOCOL: "http/json",
          OTEL_EXPORTER_OTLP_LOGS_HEADERS: "x-scope=logs%2Fonly",
          OTEL_EXPORTER_OTLP_HEADERS: "x-scope=everything",
        }),
      );
      assert.strictEqual(resolved.logs.settings?.protocol, "http/json");
      assert.deepStrictEqual(resolved.logs.settings?.headers, { "x-scope": "logs/only" });
      assert.strictEqual(resolved.traces.settings?.protocol, "http/protobuf");
      assert.deepStrictEqual(resolved.traces.settings?.headers, { "x-scope": "everything" });
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );

  it.effect("declines only the log signal that asked for grpc", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_LOGS_PROTOCOL: "grpc",
        }),
      );
      assert.strictEqual(resolved.logs.settings, undefined);
      assert.include(resolved.logs.declined ?? "", "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL");
      assert.isDefined(resolved.traces.settings);
      assert.isDefined(resolved.metrics.settings);
    }),
  );

  it.effect("exports no log records once the SDK is disabled", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_SDK_DISABLED: "true",
          OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: "https://logs.example.com/ingest",
        }),
      );
      assert.isTrue(resolved.disabled);
      assert.strictEqual(resolved.logs.settings, undefined);
      assert.strictEqual(resolved.logs.declined, undefined);
    }),
  );

  it.effect("keeps a log record delay separate from the span one", () =>
    Effect.gen(function* () {
      // The two are different variables with different defaults, and reading
      // one for the other would export log records five times slower than the
      // specification says to.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_BSP_SCHEDULE_DELAY: "9000",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 9000);
      assert.strictEqual(resolved.logs.settings?.exportIntervalMs, 1000);
    }),
  );

  it.effect("keeps exporting when an exporter name is misspelled", () =>
    Effect.gen(function* () {
      // Reading `otlpp` as "not OTLP" would turn one transposed letter into a
      // signal that stops exporting with nothing to connect the two.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_TRACES_EXPORTER: "otlpp",
        }),
      );
      assert.isDefined(resolved.traces.settings);
      assert.isTrue(resolved.warnings.some((warning) => warning.includes("OTEL_TRACES_EXPORTER")));
    }),
  );

  it.effect("stops exporting a signal that asked for an exporter this has none of", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_METRICS_EXPORTER: "prometheus",
        }),
      );
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.isDefined(resolved.traces.settings);
      assert.isTrue(resolved.warnings.some((warning) => warning.includes("OTEL_METRICS_EXPORTER")));
    }),
  );

  it.effect("says a misspelling beside otlp did nothing rather than passing it over", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_LOGS_EXPORTER: "otlp,otlpp",
        }),
      );
      assert.isDefined(resolved.logs.settings);
      assert.isTrue(resolved.warnings.some((warning) => warning.includes("otlp,otlpp")));
    }),
  );

  it.effect("says nothing about an exporter list on a signal with nowhere to go", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_TRACES_EXPORTER: "zipkin" }),
      );
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );

  it.effect("refuses a batch size of zero rather than posting one request per span", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_BSP_MAX_EXPORT_BATCH_SIZE: "0",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.maxBatchSize, 512);
      assert.isTrue(
        resolved.warnings.some((warning) => warning.includes("OTEL_BSP_MAX_EXPORT_BATCH_SIZE")),
      );
    }),
  );

  it.effect("still drains as fast as the loop allows on a delay of zero", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_BSP_SCHEDULE_DELAY: "0",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 0);
    }),
  );

  it.effect("discards a header list where one member carries no pair", () =>
    Effect.gen(function* () {
      // Keeping the readable members would authorize the stream and then route
      // it to the wrong tenant, which reads as a collector problem rather than
      // as the typo it is.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_HEADERS: "authorization=token,x-tenant",
        }),
      );
      assert.strictEqual(resolved.traces.settings?.headers, undefined);
      assert.isTrue(
        resolved.warnings.some((warning) => warning.includes("OTEL_EXPORTER_OTLP_HEADERS")),
      );
    }),
  );

  it.effect("keeps a stored endpoint from re-enabling a signal turned off by name", () =>
    Effect.gen(function* () {
      // `none` is an answer about this signal, not an absence of one, so the
      // endpoint someone saved once does not get to give the opposite answer.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_LOGS_EXPORTER: "none",
        }),
      );
      assert.strictEqual(resolved.logs.off, true);
      const logs = OtelEnvironment.resolveSignalSource({
        t3Url: undefined,
        signal: resolved.logs,
        persistedUrl: "https://stored.example.com/v1/logs",
      });
      assert.strictEqual(logs.url, undefined);
    }),
  );

  it.effect("keeps a stored endpoint from answering for a declined transport", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_TRACES_PROTOCOL: "grpc",
        }),
      );
      assert.strictEqual(resolved.traces.off, true);
      const traces = OtelEnvironment.resolveSignalSource({
        t3Url: undefined,
        signal: resolved.traces,
        persistedUrl: "https://stored.example.com/v1/traces",
      });
      assert.strictEqual(traces.url, undefined);
      assert.isDefined(traces.signal.declined);
    }),
  );

  it.effect("still reaches the endpoint T3 Code's own name gave a signal turned off", () =>
    Effect.gen(function* () {
      // T3 Code's own name outranks the standard names, so an operator who set
      // it is not overruled by a fleet-wide exporter list.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_METRICS_EXPORTER: "none",
        }),
      );
      const metrics = OtelEnvironment.resolveSignalSource({
        t3Url: "https://t3.example.com/v1/metrics",
        signal: resolved.metrics,
        persistedUrl: undefined,
      });
      assert.strictEqual(metrics.url, "https://t3.example.com/v1/metrics");
    }),
  );

  it.effect("leaves a stored endpoint alone when no standard endpoint named the signal", () =>
    Effect.gen(function* () {
      // With nowhere for these variables to send anything, the exporter list is
      // not read at all, so it says nothing about the signal and cannot switch
      // off an export it was never describing.
      const resolved = yield* OtelEnvironment.load.pipe(withEnv({ OTEL_LOGS_EXPORTER: "none" }));
      assert.strictEqual(resolved.logs.off, false);
      const logs = OtelEnvironment.resolveSignalSource({
        t3Url: undefined,
        signal: resolved.logs,
        persistedUrl: "https://stored.example.com/v1/logs",
      });
      assert.strictEqual(logs.url, "https://stored.example.com/v1/logs");
    }),
  );

  it.effect("reads a trailing comma as spacing rather than as a member", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_ENDPOINT: "https://collector.example.com",
          OTEL_EXPORTER_OTLP_HEADERS: "authorization=token,",
        }),
      );
      assert.deepStrictEqual(resolved.traces.settings?.headers, { authorization: "token" });
    }),
  );
});
