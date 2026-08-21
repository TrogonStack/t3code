/**
 * OtelEnvironment: the OpenTelemetry environment variables, read the way the
 * specification says to read them.
 *
 * T3 Code has always had its own `T3CODE_OTLP_*` names, which stay the
 * explicit answer when they are set. Everything here is the fallback for the
 * far more common case: a machine that already exports `OTEL_*` for every
 * other service on it and expects one more process to join in without being
 * told twice.
 *
 * Only the variables this server can act on are read. The exporter speaks
 * OTLP over HTTP, so `grpc` is declined loudly rather than answered with a
 * body the endpoint cannot parse, and the log signal has no exporter here at
 * all.
 *
 * @module observability/OtelEnvironment
 */
import * as Config from "effect/Config";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";

/** The wire formats this server can produce. `grpc` is not one of them. */
export type OtlpProtocol = "http/json" | "http/protobuf";

/** `OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE`. */
export type MetricsTemporality = "cumulative" | "delta";

/** Everything one signal's exporter needs, or `undefined` if it is off. */
export interface OtlpSignalSettings {
  readonly url: string;
  readonly headers: Readonly<Record<string, string>> | undefined;
  readonly exportIntervalMs: number | undefined;
  readonly maxBatchSize: number | undefined;
  readonly shutdownTimeoutMs: number | undefined;
}

export interface OtlpResourceSettings {
  readonly serviceName: string | undefined;
  readonly serviceVersion: string | undefined;
  readonly attributes: Readonly<Record<string, string>>;
}

export interface OtelEnvironment {
  /** `OTEL_SDK_DISABLED`. When set, nothing is exported by any route. */
  readonly disabled: boolean;
  readonly traces: OtlpSignalSettings | undefined;
  readonly metrics: OtlpSignalSettings | undefined;
  readonly metricsTemporality: MetricsTemporality | undefined;
  readonly resource: OtlpResourceSettings;
  /**
   * The wire format the environment asked for, or `undefined` when it said
   * nothing. The spec's default is `http/protobuf`, which applies to an
   * environment that configured OTLP through these variables; one that did not
   * keeps whatever T3 Code already used.
   */
  readonly protocol: OtlpProtocol | undefined;
  /**
   * Why a configured endpoint is not being used, if it is not. Carried rather
   * than logged here so the caller can report it once, at startup, where a
   * user is looking.
   */
  readonly declined: string | undefined;
}

const optionalString = (name: string) =>
  Config.string(name).pipe(Config.option, Config.map(Option.getOrUndefined));

const optionalInt = (name: string) =>
  Config.int(name).pipe(Config.option, Config.map(Option.getOrUndefined));

/**
 * Headers and resource attributes are a W3C Baggage string: comma separated
 * pairs, optional whitespace around each one, and percent encoded values.
 *
 * Splitting on every `=` rather than the first one truncates exactly the
 * credentials people put here, since base64 basic auth ends in `=` padding,
 * and leaving the encoding in place sends a literal `%20` as part of a bearer
 * token. Both fail as an authentication error against the collector, which
 * reads like a bad token rather than a parsing bug.
 */
const parseBaggage = (raw: string): Readonly<Record<string, string>> => {
  const entries: Record<string, string> = {};
  for (const member of raw.split(",")) {
    const separator = member.indexOf("=");
    if (separator === -1) {
      continue;
    }
    const key = member.slice(0, separator).trim();
    if (key === "") {
      continue;
    }
    const value = member.slice(separator + 1).trim();
    try {
      entries[key] = decodeURIComponent(value);
    } catch {
      entries[key] = value;
    }
  }
  return entries;
};

const optionalRecord = (name: string) =>
  optionalString(name).pipe(
    Effect.map((raw) => (raw === undefined ? undefined : parseBaggage(raw))),
  );

/**
 * `OTEL_EXPORTER_OTLP_<SIGNAL>_ENDPOINT` is a full URL and is used as given.
 * The generic `OTEL_EXPORTER_OTLP_ENDPOINT` is a base, and the spec has each
 * signal append its own path to it.
 */
const signalEndpoint = (signal: "TRACES" | "METRICS") =>
  Effect.gen(function* () {
    const specific = yield* optionalString(`OTEL_EXPORTER_OTLP_${signal}_ENDPOINT`);
    if (specific !== undefined) {
      return specific;
    }
    const base = yield* optionalString("OTEL_EXPORTER_OTLP_ENDPOINT");
    if (base === undefined) {
      return undefined;
    }
    const trimmed = base.endsWith("/") ? base.slice(0, -1) : base;
    return `${trimmed}/v1/${signal.toLowerCase()}`;
  });

/**
 * `OTEL_<SIGNAL>_EXPORTER` is a list, and `otlp` is its default. A value that
 * names other exporters and not `otlp` is a deliberate "not this one".
 */
const signalWantsOtlp = (signal: "TRACES" | "METRICS") =>
  optionalString(`OTEL_${signal}_EXPORTER`).pipe(
    Effect.map((value) => {
      if (value === undefined) {
        return true;
      }
      return value
        .split(",")
        .map((entry) => entry.trim().toLowerCase())
        .includes("otlp");
    }),
  );

const signalSettings = (signal: "TRACES" | "METRICS") =>
  Effect.gen(function* () {
    const url = yield* signalEndpoint(signal);
    if (url === undefined || !(yield* signalWantsOtlp(signal))) {
      return undefined;
    }
    const headers =
      (yield* optionalRecord(`OTEL_EXPORTER_OTLP_${signal}_HEADERS`)) ??
      (yield* optionalRecord("OTEL_EXPORTER_OTLP_HEADERS"));
    const timeoutMs =
      (yield* optionalInt(`OTEL_EXPORTER_OTLP_${signal}_TIMEOUT`)) ??
      (yield* optionalInt("OTEL_EXPORTER_OTLP_TIMEOUT")) ??
      (signal === "METRICS" ? yield* optionalInt("OTEL_METRIC_EXPORT_TIMEOUT") : undefined);
    const exportIntervalMs =
      signal === "TRACES"
        ? yield* optionalInt("OTEL_BSP_SCHEDULE_DELAY")
        : yield* optionalInt("OTEL_METRIC_EXPORT_INTERVAL");
    return {
      url,
      headers,
      exportIntervalMs,
      maxBatchSize:
        signal === "TRACES" ? yield* optionalInt("OTEL_BSP_MAX_EXPORT_BATCH_SIZE") : undefined,
      shutdownTimeoutMs: timeoutMs,
    } satisfies OtlpSignalSettings;
  });

interface ProtocolDecision {
  readonly protocol: OtlpProtocol | undefined;
  readonly declined: string | undefined;
}

/**
 * Left unset when nothing named a protocol, so a machine that never mentioned
 * OpenTelemetry keeps the wire format T3 Code has always used. `grpc` is the
 * one value that cannot be quietly downgraded: its endpoint has no
 * `/v1/traces` path and expects a framing this server does not produce, so
 * posting anything there is worse than exporting nothing.
 */
const resolveProtocol = Effect.gen(function* () {
  const raw =
    (yield* optionalString("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL")) ??
    (yield* optionalString("OTEL_EXPORTER_OTLP_PROTOCOL"));
  if (raw === undefined) {
    return { protocol: undefined, declined: undefined } satisfies ProtocolDecision;
  }
  const value = raw.trim().toLowerCase();
  if (value === "http/json" || value === "http/protobuf") {
    return { protocol: value, declined: undefined } satisfies ProtocolDecision;
  }
  return {
    protocol: undefined,
    declined: `OTEL_EXPORTER_OTLP_PROTOCOL=${value} is not supported; this server exports OTLP over HTTP only`,
  } satisfies ProtocolDecision;
});

const resolveMetricsTemporality = optionalString(
  "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
).pipe(
  Effect.map((value): MetricsTemporality | undefined => {
    const preference = value?.trim().toLowerCase();
    return preference === "delta" || preference === "cumulative" ? preference : undefined;
  }),
);

const resolveResource = Effect.gen(function* () {
  const attributes = (yield* optionalRecord("OTEL_RESOURCE_ATTRIBUTES")) ?? {};
  const {
    "service.name": attributeName,
    "service.version": attributeVersion,
    ...rest
  } = attributes;
  return {
    serviceName: (yield* optionalString("OTEL_SERVICE_NAME")) ?? attributeName,
    serviceVersion: (yield* optionalString("OTEL_SERVICE_VERSION")) ?? attributeVersion,
    attributes: rest,
  } satisfies OtlpResourceSettings;
});

/**
 * Read the environment. Never fails: a variable this server cannot honor
 * leaves the corresponding setting unset and is reported through `declined`,
 * because an unparseable telemetry knob is not a reason to refuse to start.
 */
export const load: Effect.Effect<OtelEnvironment> = Effect.gen(function* () {
  const disabled = yield* Config.boolean("OTEL_SDK_DISABLED").pipe(Config.withDefault(false));
  const { protocol, declined } = yield* resolveProtocol;
  const resource = yield* resolveResource;
  if (disabled) {
    return {
      disabled,
      traces: undefined,
      metrics: undefined,
      metricsTemporality: undefined,
      resource,
      protocol,
      declined,
    };
  }
  const unsupportedProtocol = declined !== undefined;
  return {
    disabled,
    traces: unsupportedProtocol ? undefined : yield* signalSettings("TRACES"),
    metrics: unsupportedProtocol ? undefined : yield* signalSettings("METRICS"),
    metricsTemporality: yield* resolveMetricsTemporality,
    resource,
    protocol,
    declined,
  };
}).pipe(
  Effect.catchCause((cause) =>
    Effect.logWarning("Could not read the OpenTelemetry environment", cause).pipe(
      Effect.as({
        disabled: false,
        traces: undefined,
        metrics: undefined,
        metricsTemporality: undefined,
        resource: { serviceName: undefined, serviceVersion: undefined, attributes: {} },
        protocol: undefined,
        declined: "the OpenTelemetry environment could not be read",
      }),
    ),
  ),
);

/** An environment that asked for nothing, for tests and for the pairing CLI. */
export const none: OtelEnvironment = {
  disabled: false,
  traces: undefined,
  metrics: undefined,
  metricsTemporality: undefined,
  resource: { serviceName: undefined, serviceVersion: undefined, attributes: {} },
  protocol: undefined,
  declined: undefined,
};
