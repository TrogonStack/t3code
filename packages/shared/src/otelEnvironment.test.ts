import { assert, describe, it } from "@effect/vitest";
import * as ConfigProvider from "effect/ConfigProvider";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as OtlpResource from "effect/unstable/observability/OtlpResource";

import * as OtelEnvironment from "./otelEnvironment.ts";

const withEnv = (env: Record<string, string>) =>
  Effect.provide(Layer.mergeAll(ConfigProvider.layer(ConfigProvider.fromEnv({ env }))));

const COLLECTOR = "https://collector.example.com";

/** A collector every signal can reach, plus whatever the case under test adds. */
const read = (env: Record<string, string> = {}) =>
  OtelEnvironment.load.pipe(withEnv({ OTEL_EXPORTER_OTLP_ENDPOINT: COLLECTOR, ...env }));

const warnings = (resolved: OtelEnvironment.OtelEnvironment) => resolved.warnings.join("\n");

describe("OtelEnvironment", () => {
  it.effect("stays off when nothing is configured", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(withEnv({}));
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.strictEqual(resolved.logs.settings, undefined);
      assert.isFalse(resolved.disabled);
    }),
  );

  it.effect("reads the specification's own defaults for a collector named this way", () =>
    Effect.gen(function* () {
      // Once these variables are the ones configuring the exporter, the numbers
      // that apply are the specification's and not the ones T3 Code picked for
      // itself, and an aggregation nobody asked for is left for the exporter.
      const resolved = yield* read();
      assert.strictEqual(resolved.traces.settings?.url, `${COLLECTOR}/v1/traces`);
      assert.strictEqual(resolved.metrics.settings?.url, `${COLLECTOR}/v1/metrics`);
      assert.strictEqual(resolved.logs.settings?.url, `${COLLECTOR}/v1/logs`);
      assert.strictEqual(resolved.traces.settings?.protocol, "http/protobuf");
      assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 5000);
      assert.strictEqual(resolved.traces.settings?.maxBatchSize, 512);
      assert.strictEqual(resolved.metrics.settings?.exportIntervalMs, 60_000);
      assert.strictEqual(resolved.metrics.settings?.temporality, undefined);
      assert.strictEqual(resolved.logs.settings?.exportIntervalMs, 1000);
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );

  const ENDPOINT_JOINS = [
    { given: "a bare host", endpoint: COLLECTOR, url: `${COLLECTOR}/v1/traces` },
    { given: "a trailing slash", endpoint: `${COLLECTOR}/`, url: `${COLLECTOR}/v1/traces` },
    { given: "a base path", endpoint: `${COLLECTOR}/otel`, url: `${COLLECTOR}/otel/v1/traces` },
    {
      // Several vendor intakes take their API key in the query string, where
      // joining the strings would make the path part of the key's value.
      given: "a query string",
      endpoint: "https://intake.example.com/otlp?key=abc",
      url: "https://intake.example.com/otlp/v1/traces?key=abc",
    },
    {
      // A shell profile that lined up its exports did not mean the padding to
      // land in the middle of the URL, where nothing would report it.
      given: "padding",
      endpoint: `  ${COLLECTOR}/  `,
      url: `${COLLECTOR}/v1/traces`,
    },
  ];

  for (const join of ENDPOINT_JOINS) {
    it.effect(`appends the signal path to a generic endpoint with ${join.given}`, () =>
      Effect.gen(function* () {
        const resolved = yield* OtelEnvironment.load.pipe(
          withEnv({ OTEL_EXPORTER_OTLP_ENDPOINT: join.endpoint }),
        );
        assert.strictEqual(resolved.traces.settings?.url, join.url);
      }),
    );
  }

  const UNUSABLE = [
    {
      given: "a disable flag the specification does not define",
      env: { OTEL_SDK_DISABLED: "yes" },
      named: "OTEL_SDK_DISABLED=yes",
      check: (resolved: OtelEnvironment.OtelEnvironment) => {
        assert.isFalse(resolved.disabled);
        assert.isDefined(resolved.traces.settings);
      },
    },
    {
      given: "a delay that is not a number",
      env: { OTEL_BSP_SCHEDULE_DELAY: "abc" },
      named: "OTEL_BSP_SCHEDULE_DELAY=abc",
      check: (resolved: OtelEnvironment.OtelEnvironment) => {
        assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 5000);
        assert.isDefined(resolved.metrics.settings);
      },
    },
    {
      given: "a misspelled wire format",
      env: { OTEL_EXPORTER_OTLP_PROTOCOL: "htp/json" },
      named: "htp/json",
      check: (resolved: OtelEnvironment.OtelEnvironment) => {
        assert.strictEqual(resolved.traces.settings?.protocol, "http/protobuf");
        assert.strictEqual(resolved.traces.declined, undefined);
      },
    },
    {
      given: "an aggregation that is not a preference",
      env: { OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: "hourly" },
      named: "hourly",
      check: (resolved: OtelEnvironment.OtelEnvironment) => {
        assert.isDefined(resolved.metrics.settings);
        assert.strictEqual(resolved.metrics.settings?.temporality, undefined);
      },
    },
    {
      // Half a header set is worse than none: the collector answers a partial
      // credential with the same 401 it gives a wrong one, and routes the
      // stream to whichever tenant the readable members named.
      given: "a header list with a member that carries no pair",
      env: { OTEL_EXPORTER_OTLP_HEADERS: "authorization=token,x-tenant" },
      named: "OTEL_EXPORTER_OTLP_HEADERS",
      check: (resolved: OtelEnvironment.OtelEnvironment) =>
        assert.strictEqual(resolved.traces.settings?.headers, undefined),
    },
    {
      given: "a header value that is not valid percent encoding",
      env: { OTEL_EXPORTER_OTLP_HEADERS: "x-token=100%zz,x-other=100%25" },
      named: "OTEL_EXPORTER_OTLP_HEADERS",
      check: (resolved: OtelEnvironment.OtelEnvironment) =>
        assert.strictEqual(resolved.traces.settings?.headers, undefined),
    },
    {
      given: "a resource attribute that does not decode",
      env: { OTEL_RESOURCE_ATTRIBUTES: "team=100%zz,deployment=prod" },
      named: "OTEL_RESOURCE_ATTRIBUTES",
      check: (resolved: OtelEnvironment.OtelEnvironment) =>
        assert.deepStrictEqual(resolved.resourceAttributes, {}),
    },
  ];

  for (const unusable of UNUSABLE) {
    it.effect(`keeps exporting and reports ${unusable.given}`, () =>
      Effect.gen(function* () {
        const resolved = yield* read(unusable.env);
        unusable.check(resolved);
        assert.include(warnings(resolved), unusable.named);
      }),
    );
  }

  it.effect("treats an empty value as an unset one", () =>
    Effect.gen(function* () {
      const resolved = yield* read({
        OTEL_SERVICE_NAME: "",
        OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: "",
      });
      // An empty rename is not a rename, so it is not worth a warning either.
      assert.deepStrictEqual(resolved.warnings, []);
      assert.strictEqual(resolved.traces.settings?.url, `${COLLECTOR}/v1/traces`);
    }),
  );

  const HEADER_LISTS = [
    {
      given: "the percent encoding the specification asks for",
      value: "Authorization=Bearer%20abc123, x-scope=team%2Fplatform",
      headers: { Authorization: "Bearer abc123", "x-scope": "team/platform" },
    },
    {
      given: "a credential that contains its own separator",
      value: "Authorization=Basic YWJjOmRlZg==",
      headers: { Authorization: "Basic YWJjOmRlZg==" },
    },
    {
      given: "a trailing comma as spacing rather than as a member",
      value: "authorization=token,",
      headers: { authorization: "token" },
    },
  ];

  for (const list of HEADER_LISTS) {
    it.effect(`reads a header list written with ${list.given}`, () =>
      Effect.gen(function* () {
        const resolved = yield* read({ OTEL_EXPORTER_OTLP_HEADERS: list.value });
        assert.deepStrictEqual(resolved.traces.settings?.headers, list.headers);
      }),
    );
  }

  it.effect("reads the service version and the resource attributes beside it", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_RESOURCE_ATTRIBUTES: "team=platform%20eng,deployment.environment=prod",
          OTEL_SERVICE_VERSION: "  1.2.3  ",
        }),
      );
      assert.strictEqual(resolved.serviceVersion, "1.2.3");
      // service.version becomes a named field, so leaving it in the attribute
      // bag too would send it twice.
      assert.deepStrictEqual(resolved.resourceAttributes, {
        team: "platform eng",
        "deployment.environment": "prod",
      });
    }),
  );

  const SERVICE_NAME_REFUSALS = [
    {
      given: "OTEL_SERVICE_NAME",
      env: { OTEL_SERVICE_NAME: "some-other-app" },
      attributes: {},
      named: "OTEL_SERVICE_NAME was ignored",
    },
    {
      given: "a service.name hidden in the resource attributes",
      env: { OTEL_RESOURCE_ATTRIBUTES: "service.name=some-other-app,host.name=lab-01" },
      attributes: { "host.name": "lab-01" },
      named: "service.name was ignored",
    },
    {
      given: "a percent-encoded service.name",
      env: { OTEL_RESOURCE_ATTRIBUTES: "team%2Fname=blue,service%2Ename=impostor" },
      attributes: { "team/name": "blue" },
      named: "service.name was ignored",
    },
    {
      given: "both names at once",
      env: {
        OTEL_SERVICE_NAME: "explicit",
        OTEL_RESOURCE_ATTRIBUTES: "service.name=from-attributes",
      },
      attributes: {},
      named: "OTEL_SERVICE_NAME was ignored",
    },
  ];

  for (const refusal of SERVICE_NAME_REFUSALS) {
    it.effect(`refuses to rename the service through ${refusal.given}`, () =>
      Effect.gen(function* () {
        // Each T3 Code process names itself, so a fleet-wide rename would merge
        // two of them into one service, and passing the attribute through would
        // send a second service.name beside the one the process chose.
        const resolved = yield* OtelEnvironment.load.pipe(withEnv(refusal.env));
        assert.deepStrictEqual(resolved.resourceAttributes, refusal.attributes);
        assert.lengthOf(resolved.warnings, 1);
        assert.include(resolved.warnings[0] ?? "", refusal.named);
      }),
    );
  }

  it.effect("refuses a zero interval and a zero batch size on every signal", () =>
    Effect.gen(function* () {
      // The exporter sleeps for the interval before each run, so zero is a busy
      // loop, and a zero batch posts one request per span.
      const resolved = yield* read({
        OTEL_BSP_SCHEDULE_DELAY: "0",
        OTEL_METRIC_EXPORT_INTERVAL: "0",
        OTEL_BLRP_SCHEDULE_DELAY: "0",
        OTEL_BSP_MAX_EXPORT_BATCH_SIZE: "0",
        OTEL_BLRP_MAX_EXPORT_BATCH_SIZE: "0",
      });
      assert.strictEqual(resolved.traces.settings?.exportIntervalMs, 5000);
      assert.strictEqual(resolved.metrics.settings?.exportIntervalMs, 60_000);
      assert.strictEqual(resolved.logs.settings?.exportIntervalMs, 1000);
      assert.strictEqual(resolved.traces.settings?.maxBatchSize, 512);
      assert.strictEqual(resolved.logs.settings?.maxBatchSize, 512);
      for (const name of [
        "OTEL_BSP_SCHEDULE_DELAY",
        "OTEL_METRIC_EXPORT_INTERVAL",
        "OTEL_BLRP_SCHEDULE_DELAY",
        "OTEL_BSP_MAX_EXPORT_BATCH_SIZE",
        "OTEL_BLRP_MAX_EXPORT_BATCH_SIZE",
      ]) {
        assert.include(warnings(resolved), `${name}=0`);
      }
    }),
  );

  it.effect("declines gRPC on the signal that asked for it and leaves the others alone", () =>
    Effect.gen(function* () {
      // A metric endpoint that speaks gRPC says nothing about where traces go,
      // and switching traces off over it loses telemetry nobody asked to lose.
      const resolved = yield* read({ OTEL_EXPORTER_OTLP_METRICS_PROTOCOL: "grpc" });
      assert.isDefined(resolved.traces.settings);
      assert.isDefined(resolved.logs.settings);
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.include(resolved.metrics.declined ?? "", "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL");
      assert.isTrue(resolved.metrics.off);
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
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );

  const EXPORTER_LISTS = [
    { given: "none", value: "none", exports: false, reported: false },
    // Reading `otlpp` as "not OTLP" would turn one transposed letter into a
    // signal that silently stops exporting.
    { given: "a misspelling of otlp", value: "otlpp", exports: true, reported: true },
    {
      given: "otlp beside an exporter T3 Code has none of",
      value: "console, otlp",
      exports: true,
      reported: true,
    },
    // A real exporter for this signal that this build does not have, so the
    // operator asked for something deliberate that cannot be served here.
    { given: "an exporter T3 Code has none of", value: "zipkin", exports: false, reported: true },
  ];

  for (const list of EXPORTER_LISTS) {
    it.effect(`reads an exporter list of ${list.given}`, () =>
      Effect.gen(function* () {
        const resolved = yield* read({ OTEL_TRACES_EXPORTER: list.value });
        assert.strictEqual(resolved.traces.settings === undefined, !list.exports);
        assert.strictEqual(warnings(resolved).includes("OTEL_TRACES_EXPORTER"), list.reported);
        assert.isDefined(resolved.metrics.settings);
      }),
    );
  }

  it.effect("says nothing about an exporter list on a signal with nowhere to go", () =>
    Effect.gen(function* () {
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({ OTEL_TRACES_EXPORTER: "zipkin" }),
      );
      assert.deepStrictEqual(resolved.warnings, []);
    }),
  );

  it.effect("resolves lowmemory to the aggregation it asks for on these metrics", () =>
    Effect.gen(function* () {
      // Falling back to the default here would invert the request rather than
      // decline it, and invert it toward the value a delta-only receiver drops
      // without an error, so the timers would vanish and the counters would not.
      const resolved = yield* read({
        OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: "lowmemory",
      });
      assert.strictEqual(resolved.metrics.settings?.temporality, "delta");
      assert.include(warnings(resolved), "lowmemory");
    }),
  );

  it.effect("exports nothing at all when the SDK is disabled", () =>
    Effect.gen(function* () {
      const resolved = yield* read({ OTEL_SDK_DISABLED: "true" });
      assert.isTrue(resolved.disabled);
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.strictEqual(resolved.logs.settings, undefined);
      // Someone who inherited this from a shell profile has somewhere to go.
      assert.include(warnings(resolved), "T3CODE_OTEL_SDK_DISABLED=false");
    }),
  );

  it.effect("lets T3 Code's own name answer before the standard one", () =>
    Effect.gen(function* () {
      const off = yield* read({ T3CODE_OTEL_SDK_DISABLED: "true" });
      assert.isTrue(off.disabled);
      assert.strictEqual(off.traces.settings, undefined);
      assert.deepStrictEqual(off.warnings, [
        "T3CODE_OTEL_SDK_DISABLED is set, so no telemetry is exported, whatever configured it",
      ]);

      // The point of reading ours first: a machine that disables every other
      // SDK in its shell profile can still ask for T3 Code's telemetry.
      const on = yield* read({ T3CODE_OTEL_SDK_DISABLED: "false", OTEL_SDK_DISABLED: "true" });
      assert.isFalse(on.disabled);
      assert.isDefined(on.traces.settings);
      assert.deepStrictEqual(on.warnings, []);
    }),
  );

  it.effect("reads T3 Code's own name the way T3 Code reads a boolean", () =>
    Effect.gen(function* () {
      // Ours to define, so it takes the affirmatives people type. The
      // specification's single spelling stays with the OTEL_* name.
      const numeric = yield* OtelEnvironment.load.pipe(withEnv({ T3CODE_OTEL_SDK_DISABLED: "1" }));
      assert.isTrue(numeric.disabled);

      // A value that answers nothing leaves the source under it to answer.
      const nonsense = yield* read({
        T3CODE_OTEL_SDK_DISABLED: "maybe",
        OTEL_SDK_DISABLED: "true",
      });
      assert.isTrue(nonsense.disabled);
      assert.include(warnings(nonsense), "T3CODE_OTEL_SDK_DISABLED=maybe");
    }),
  );

  it.effect("takes a signal endpoint exactly as written and leaves the others generic", () =>
    Effect.gen(function* () {
      // The per-signal variable is a whole URL. Appending to it would send
      // traces to a path the collector does not serve.
      const resolved = yield* read({
        OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: "https://traces.example.com/ingest",
      });
      assert.strictEqual(resolved.traces.settings?.url, "https://traces.example.com/ingest");
      assert.strictEqual(resolved.metrics.settings?.url, `${COLLECTOR}/v1/metrics`);
    }),
  );

  it.effect("keeps a signal's own endpoint from falling back to the generic collector", () =>
    Effect.gen(function* () {
      // A variable that names this signal owns it once it is set at all, so a
      // value nobody can use is not an invitation to post the signal somewhere
      // else that was configured for the other signals.
      const resolved = yield* read({
        OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: "collector.example.com",
      });
      assert.strictEqual(resolved.traces.settings, undefined);
      assert.strictEqual(resolved.metrics.settings?.url, `${COLLECTOR}/v1/metrics`);
      assert.include(warnings(resolved), "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
      assert.include(warnings(resolved), "OTEL_EXPORTER_OTLP_ENDPOINT was not used in its place");
    }),
  );

  it.effect("replaces the generic headers with the signal's own rather than merging", () =>
    Effect.gen(function* () {
      // What the specification says, and what a collector holding two different
      // keys depends on.
      const resolved = yield* read({
        OTEL_EXPORTER_OTLP_HEADERS: "api-key=abc123,x-tenant=acme",
        OTEL_EXPORTER_OTLP_TRACES_HEADERS: "api-key=traces-only",
      });
      assert.deepStrictEqual(resolved.traces.settings?.headers, { "api-key": "traces-only" });
      assert.deepStrictEqual(resolved.metrics.settings?.headers, {
        "api-key": "abc123",
        "x-tenant": "acme",
      });
    }),
  );

  it.effect("keeps a signal's own header list from sending the generic credential", () =>
    Effect.gen(function* () {
      // The signal's list is malformed rather than absent, and the generic
      // credential belongs to whoever was told to accept it, so falling back
      // would authorize this stream as a tenant nobody named for it.
      const resolved = yield* read({
        OTEL_EXPORTER_OTLP_TRACES_HEADERS: "junk",
        OTEL_EXPORTER_OTLP_HEADERS: "Authorization=Bearer%20abc123",
      });
      assert.strictEqual(resolved.traces.settings?.headers, undefined);
      assert.deepStrictEqual(resolved.metrics.settings?.headers, {
        Authorization: "Bearer abc123",
      });
      assert.include(warnings(resolved), "OTEL_EXPORTER_OTLP_TRACES_HEADERS");
      assert.include(warnings(resolved), "OTEL_EXPORTER_OTLP_HEADERS was not used in its place");
    }),
  );

  it.effect("does not write the credential it refused into the startup log", () =>
    Effect.gen(function* () {
      // These warnings are logged, so a variable that carries a token is named
      // without quoting what was in it.
      const resolved = yield* read({
        OTEL_EXPORTER_OTLP_HEADERS: "authorization=Bearer%20super-secret-token,x-tenant",
        OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: "https://someone:hunter2@collector.example.com:port",
      });

      assert.include(warnings(resolved), "OTEL_EXPORTER_OTLP_HEADERS");
      assert.include(warnings(resolved), "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT");
      assert.notInclude(warnings(resolved), "super-secret-token");
      assert.notInclude(warnings(resolved), "hunter2");
    }),
  );

  it.effect("names a shared variable once even though every signal reads it", () =>
    Effect.gen(function* () {
      const resolved = yield* read({ OTEL_EXPORTER_OTLP_HEADERS: "x-token=100%zz" });
      assert.lengthOf(
        resolved.warnings.filter((warning) => warning.includes("OTEL_EXPORTER_OTLP_HEADERS")),
        1,
      );
    }),
  );

  it.effect("says nothing about an aggregation for metrics these variables did not place", () =>
    Effect.gen(function* () {
      // The preference travels with the endpoint that asked for it, so on a
      // machine whose metrics endpoint comes from somewhere else this warning
      // would claim an aggregation that never applied.
      const resolved = yield* OtelEnvironment.load.pipe(
        withEnv({
          OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: `${COLLECTOR}/v1/traces`,
          OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: "lowmemory",
        }),
      );
      assert.strictEqual(resolved.metrics.settings, undefined);
      assert.notInclude(warnings(resolved), "lowmemory");
    }),
  );

  it("does not lend T3 Code's own settings to a signal the standard names configured", () => {
    const settings = {
      url: `${COLLECTOR}/v1/traces`,
      protocol: "http/protobuf",
      headers: undefined,
      exportIntervalMs: 5000,
      maxBatchSize: 512,
      temporality: undefined,
    } as const;
    const t3 = {
      t3Protocol: "http/json",
      t3Headers: { authorization: "t3-only" },
      t3ExportIntervalMs: 10_000,
    } as const;

    // Settings that exist and say nothing about the headers are these variables
    // owning the signal and being silent, which is an answer of its own.
    const standard = OtelEnvironment.resolveSignalExport({ settings, ...t3 });
    assert.strictEqual(standard.headers, undefined);
    assert.strictEqual(standard.protocol, "http/protobuf");
    assert.strictEqual(standard.exportIntervalMs, 5000);

    // Absent settings mean the standard variables named nothing, so T3 Code's
    // own answers apply.
    const own = OtelEnvironment.resolveSignalExport({ settings: undefined, ...t3 });
    assert.deepStrictEqual(own.headers, { authorization: "t3-only" });
    assert.strictEqual(own.protocol, "http/json");
    assert.strictEqual(own.exportIntervalMs, 10_000);
  });

  it.effect("lets T3 Code's own endpoint reach a signal the standard names turned off", () =>
    Effect.gen(function* () {
      // T3 Code's own name outranks the standard ones, so an operator who set
      // it is not overruled by a fleet-wide exporter list.
      const resolved = yield* read({ OTEL_METRICS_EXPORTER: "none" });
      const metrics = OtelEnvironment.resolveSignalSource({
        t3Url: "https://t3.example.com/v1/metrics",
        signal: resolved.metrics,
        persistedUrl: undefined,
      });
      assert.strictEqual(metrics.url, "https://t3.example.com/v1/metrics");
      assert.strictEqual(metrics.signal.settings, undefined);
    }),
  );

  it.effect("keeps a stored endpoint from re-enabling a signal turned off by name", () =>
    Effect.gen(function* () {
      // `none` is an answer about this signal, not an absence of one, so the
      // endpoint somebody saved once does not get to give the opposite answer.
      const resolved = yield* read({ OTEL_LOGS_EXPORTER: "none" });
      assert.isTrue(resolved.logs.off);
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
      const resolved = yield* read({ OTEL_EXPORTER_OTLP_TRACES_PROTOCOL: "grpc" });
      const traces = OtelEnvironment.resolveSignalSource({
        t3Url: undefined,
        signal: resolved.traces,
        persistedUrl: "https://stored.example.com/v1/traces",
      });
      assert.strictEqual(traces.url, undefined);
      assert.isDefined(traces.signal.declined);
    }),
  );

  it.effect("leaves a stored endpoint alone when no standard endpoint named the signal", () =>
    Effect.gen(function* () {
      // With nowhere for these variables to send anything, the exporter list is
      // not read at all, so it cannot switch off an export it never described.
      const resolved = yield* OtelEnvironment.load.pipe(withEnv({ OTEL_LOGS_EXPORTER: "none" }));
      assert.isFalse(resolved.logs.off);
      const logs = OtelEnvironment.resolveSignalSource({
        t3Url: undefined,
        signal: resolved.logs,
        persistedUrl: "https://stored.example.com/v1/logs",
      });
      assert.strictEqual(logs.url, "https://stored.example.com/v1/logs");
      assert.strictEqual(logs.signal.settings, undefined);
    }),
  );
});

describe("OtelEnvironment kill switch", () => {
  const load = (env: Record<string, string>) =>
    OtelEnvironment.load.pipe(
      Effect.provide(ConfigProvider.layer(ConfigProvider.fromEnv({ env }))),
    );

  const SPEC_OFF =
    "OTEL_SDK_DISABLED is set, so no telemetry is exported, whatever configured it; set T3CODE_OTEL_SDK_DISABLED=false to export anyway";
  const T3_OFF =
    "T3CODE_OTEL_SDK_DISABLED is set, so no telemetry is exported, whatever configured it";
  const specIgnored = (value: string) =>
    `OTEL_SDK_DISABLED=${value} was read as false; the OpenTelemetry specification recognizes only the string true, so use OTEL_SDK_DISABLED=true or T3CODE_OTEL_SDK_DISABLED to say it any other way`;

  it.effect.each([
    { name: "nothing set", env: {}, disabled: false, warnings: [] },
    // OTEL_SDK_DISABLED follows the specification: only `true`, case-insensitively.
    { name: "spec true", env: { OTEL_SDK_DISABLED: "true" }, disabled: true, warnings: [SPEC_OFF] },
    { name: "spec True", env: { OTEL_SDK_DISABLED: "True" }, disabled: true, warnings: [SPEC_OFF] },
    {
      name: "spec padded",
      env: { OTEL_SDK_DISABLED: " true " },
      disabled: true,
      warnings: [SPEC_OFF],
    },
    { name: "spec false", env: { OTEL_SDK_DISABLED: "false" }, disabled: false, warnings: [] },
    {
      name: "spec 1",
      env: { OTEL_SDK_DISABLED: "1" },
      disabled: false,
      warnings: [specIgnored("1")],
    },
    {
      name: "spec padded yes",
      env: { OTEL_SDK_DISABLED: " yes " },
      disabled: false,
      warnings: [specIgnored("yes")],
    },
    // T3CODE_OTEL_SDK_DISABLED takes Config.Boolean's values, case-insensitively.
    { name: "t3 1", env: { T3CODE_OTEL_SDK_DISABLED: "1" }, disabled: true, warnings: [T3_OFF] },
    {
      name: "t3 TRUE",
      env: { T3CODE_OTEL_SDK_DISABLED: "TRUE" },
      disabled: true,
      warnings: [T3_OFF],
    },
    { name: "t3 n", env: { T3CODE_OTEL_SDK_DISABLED: "n" }, disabled: false, warnings: [] },
    {
      name: "t3 false overrides spec true",
      env: { T3CODE_OTEL_SDK_DISABLED: "false", OTEL_SDK_DISABLED: "true" },
      disabled: false,
      warnings: [],
    },
    {
      name: "blank t3 falls through",
      env: { T3CODE_OTEL_SDK_DISABLED: "  ", OTEL_SDK_DISABLED: "true" },
      disabled: true,
      warnings: [SPEC_OFF],
    },
    {
      name: "unreadable t3 warns and falls through",
      env: { T3CODE_OTEL_SDK_DISABLED: "maybe", OTEL_SDK_DISABLED: "true" },
      disabled: true,
      warnings: ["T3CODE_OTEL_SDK_DISABLED=maybe is not a yes or a no and was ignored", SPEC_OFF],
    },
    {
      name: "bad spec value still warns when t3 answered",
      env: { T3CODE_OTEL_SDK_DISABLED: "false", OTEL_SDK_DISABLED: "yes" },
      disabled: false,
      warnings: [specIgnored("yes")],
    },
  ])("$name", ({ env, disabled, warnings }) =>
    Effect.gen(function* () {
      const resolved = yield* load(env);
      assert.strictEqual(resolved.disabled, disabled);
      assert.deepStrictEqual(resolved.warnings, warnings);
    }),
  );

  it.effect.each([
    { name: "unset", env: {}, resourceAttributes: {}, warnings: [] },
    {
      name: "a percent-encoded list",
      env: { OTEL_RESOURCE_ATTRIBUTES: "team=core,message=hello%20world" },
      resourceAttributes: { team: "core", message: "hello world" },
      warnings: [],
    },
    {
      name: "a list that does not decode",
      env: { OTEL_RESOURCE_ATTRIBUTES: "team=core,broken=%zz" },
      resourceAttributes: {},
      warnings: [
        "OTEL_RESOURCE_ATTRIBUTES is not a list of percent-encoded key=value pairs and was ignored",
      ],
    },
  ])("resource attributes: $name", ({ env, resourceAttributes, warnings }) =>
    Effect.gen(function* () {
      const resolved = yield* load(env);
      assert.deepStrictEqual(resolved.resourceAttributes, resourceAttributes);
      assert.deepStrictEqual(resolved.warnings, warnings);
    }),
  );

  describe("layerResourceAttributes", () => {
    it.effect.each([
      { name: "a list that does not decode", raw: "team=%zz", attributes: [] },
      { name: "encoded separators", raw: "a%2Cb=x%3Dy", attributes: ["a,b"] },
    ])("lets the exporters' own read succeed with $name", ({ raw, attributes }) =>
      Effect.gen(function* () {
        const env = ConfigProvider.layer(
          ConfigProvider.fromEnv({ env: { OTEL_RESOURCE_ATTRIBUTES: raw } }),
        );
        const otel = yield* OtelEnvironment.load.pipe(Effect.provide(env));
        const resource = yield* OtlpResource.fromConfig({ serviceName: "t3" }).pipe(
          Effect.provide(
            Layer.provide(OtelEnvironment.layerResourceAttributes(otel.resourceAttributes), env),
          ),
        );
        assert.deepStrictEqual(
          resource.attributes.map((attribute) => attribute.key),
          [...attributes, "service.name"],
        );
      }),
    );
  });
});
