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
 * Everything else the specification requires of an unusable value is a
 * warning followed by the default, never a refusal to start and never a
 * silently different behavior.
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
  /**
   * Settings that were named but could not be used, each already phrased for a
   * human. The specification requires a warning for a value the implementation
   * does not recognize, and these are collected rather than logged here so the
   * caller reports them once, at startup, where someone is looking.
   */
  readonly warnings: ReadonlyArray<string>;
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
const parseBaggage = (raw: string): Readonly<Record<string, string>> | undefined => {
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
      return undefined;
    }
  }
  return entries;
};

interface Parsed<A> {
  readonly value: A | undefined;
  readonly warning: string | undefined;
}

/**
 * A pair list that fails to decode discards the whole variable, which is what
 * the resource specification asks for and the safer answer for headers too: a
 * half-parsed credential reaches the collector as an authentication error,
 * while nothing plus a warning says where to look.
 */
const optionalRecord = (name: string) =>
  optionalString(name).pipe(
    Effect.map((raw) => {
      if (raw === undefined) {
        return { value: undefined, warning: undefined };
      }
      const parsed = parseBaggage(raw);
      return parsed === undefined
        ? { value: undefined, warning: `${name} is not valid percent encoding and was ignored` }
        : { value: parsed, warning: undefined };
    }),
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

/**
 * The specification's own defaults, which apply once this route is the one
 * configuring the exporter. A `T3CODE_OTLP_*` setup never reaches here and
 * keeps the numbers T3 Code has always used.
 */
const SPEC_DEFAULT_SCHEDULE_DELAY_MS = 5_000;
const SPEC_DEFAULT_MAX_EXPORT_BATCH_SIZE = 512;
const SPEC_DEFAULT_METRIC_EXPORT_INTERVAL_MS = 60_000;

const signalSettings = (signal: "TRACES" | "METRICS") =>
  Effect.gen(function* () {
    const url = yield* signalEndpoint(signal);
    if (url === undefined || !(yield* signalWantsOtlp(signal))) {
      return { value: undefined, warning: undefined };
    }
    const specific = yield* optionalRecord(`OTEL_EXPORTER_OTLP_${signal}_HEADERS`);
    const generic = yield* optionalRecord("OTEL_EXPORTER_OTLP_HEADERS");
    const headers = specific.value ?? generic.value;
    const timeoutMs =
      (yield* optionalInt(`OTEL_EXPORTER_OTLP_${signal}_TIMEOUT`)) ??
      (yield* optionalInt("OTEL_EXPORTER_OTLP_TIMEOUT")) ??
      (signal === "METRICS" ? yield* optionalInt("OTEL_METRIC_EXPORT_TIMEOUT") : undefined);
    const exportIntervalMs =
      signal === "TRACES"
        ? ((yield* optionalInt("OTEL_BSP_SCHEDULE_DELAY")) ?? SPEC_DEFAULT_SCHEDULE_DELAY_MS)
        : ((yield* optionalInt("OTEL_METRIC_EXPORT_INTERVAL")) ??
          SPEC_DEFAULT_METRIC_EXPORT_INTERVAL_MS);
    return {
      value: {
        url,
        headers,
        exportIntervalMs,
        maxBatchSize:
          signal === "TRACES"
            ? ((yield* optionalInt("OTEL_BSP_MAX_EXPORT_BATCH_SIZE")) ??
              SPEC_DEFAULT_MAX_EXPORT_BATCH_SIZE)
            : undefined,
        shutdownTimeoutMs: timeoutMs,
      },
      warning: specific.warning ?? generic.warning,
    } satisfies Parsed<OtlpSignalSettings>;
  });

interface ProtocolDecision {
  readonly protocol: OtlpProtocol | undefined;
  readonly declined: string | undefined;
  readonly warnings: ReadonlyArray<string>;
}

/**
 * Left unset when nothing named a protocol, so a machine that never mentioned
 * OpenTelemetry keeps the wire format T3 Code has always used.
 *
 * `grpc` is the one value that turns export off rather than falling back. It
 * is a real protocol this server does not speak, its endpoint has no
 * `/v1/traces` path, and it expects a framing nothing here produces, so
 * posting to it is worse than exporting nothing. A value that is not a
 * protocol at all is a typo, and the specification is explicit that those get
 * a warning and the default.
 *
 * One serializer covers both signals, so a per-signal protocol that disagrees
 * with the trace protocol cannot be honored and says so.
 */
const resolveProtocol = Effect.gen(function* () {
  const warnings: Array<string> = [];
  const read = function* (name: string) {
    const raw = yield* optionalString(name);
    if (raw === undefined) {
      return undefined;
    }
    const value = raw.trim().toLowerCase();
    if (value === "http/json" || value === "http/protobuf" || value === "grpc") {
      return value;
    }
    warnings.push(`${name}=${raw} is not a known OTLP protocol and was ignored`);
    return undefined;
  };

  const traces =
    (yield* read("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL")) ??
    (yield* read("OTEL_EXPORTER_OTLP_PROTOCOL"));
  const metrics = yield* read("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL");
  if (metrics !== undefined && traces !== undefined && metrics !== traces) {
    warnings.push(
      `OTEL_EXPORTER_OTLP_METRICS_PROTOCOL=${metrics} cannot differ from the trace protocol here; ${traces} is used for both`,
    );
  }
  const chosen = traces ?? metrics;
  if (chosen === "grpc") {
    return {
      protocol: undefined,
      declined:
        "OTEL_EXPORTER_OTLP_PROTOCOL=grpc is not supported; this server exports OTLP over HTTP only, so nothing is exported",
      warnings,
    } satisfies ProtocolDecision;
  }
  return { protocol: chosen, declined: undefined, warnings } satisfies ProtocolDecision;
});

/**
 * `lowmemory` is a real preference in the specification that this exporter
 * cannot produce, so it warns and falls back to the default rather than
 * pretending it applied.
 */
const resolveMetricsTemporality = optionalString(
  "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
).pipe(
  Effect.map((raw): Parsed<MetricsTemporality> => {
    if (raw === undefined) {
      return { value: undefined, warning: undefined };
    }
    const preference = raw.trim().toLowerCase();
    if (preference === "delta" || preference === "cumulative") {
      return { value: preference, warning: undefined };
    }
    return {
      value: undefined,
      warning:
        preference === "lowmemory"
          ? "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE=lowmemory is not supported here; cumulative is used"
          : `OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE=${raw} is not a known preference and was ignored`,
    };
  }),
);

const resolveResource = Effect.gen(function* () {
  const parsed = yield* optionalRecord("OTEL_RESOURCE_ATTRIBUTES");
  const {
    "service.name": attributeName,
    "service.version": attributeVersion,
    ...rest
  } = parsed.value ?? {};
  return {
    value: {
      serviceName: (yield* optionalString("OTEL_SERVICE_NAME")) ?? attributeName,
      serviceVersion: (yield* optionalString("OTEL_SERVICE_VERSION")) ?? attributeVersion,
      attributes: rest,
    },
    warning: parsed.warning,
  } satisfies Parsed<OtlpResourceSettings>;
});

/**
 * Read the environment. Never fails: a variable this server cannot honor
 * leaves the corresponding setting unset and is reported through `declined`,
 * because an unparseable telemetry knob is not a reason to refuse to start.
 */
export const load: Effect.Effect<OtelEnvironment> = Effect.gen(function* () {
  const disabled = yield* Config.boolean("OTEL_SDK_DISABLED").pipe(Config.withDefault(false));
  const protocolDecision = yield* resolveProtocol;
  const resource = yield* resolveResource;
  const temporality = yield* resolveMetricsTemporality;
  const traces = disabled
    ? { value: undefined, warning: undefined }
    : yield* signalSettings("TRACES");
  const metrics = disabled
    ? { value: undefined, warning: undefined }
    : yield* signalSettings("METRICS");
  const exportable = !disabled && protocolDecision.declined === undefined;
  return {
    disabled,
    warnings: [
      ...protocolDecision.warnings,
      resource.warning,
      temporality.warning,
      traces.warning,
      metrics.warning,
    ].filter((warning) => warning !== undefined),
    traces: exportable ? traces.value : undefined,
    metrics: exportable ? metrics.value : undefined,
    metricsTemporality: temporality.value,
    resource: resource.value,
    protocol: protocolDecision.protocol,
    declined: protocolDecision.declined,
  };
}).pipe(
  Effect.catchCause((cause) =>
    Effect.logWarning("Could not read the OpenTelemetry environment", cause).pipe(
      Effect.as({
        disabled: false,
        warnings: [],
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
  warnings: [],
  traces: undefined,
  metrics: undefined,
  metricsTemporality: undefined,
  resource: { serviceName: undefined, serviceVersion: undefined, attributes: {} },
  protocol: undefined,
  declined: undefined,
};
