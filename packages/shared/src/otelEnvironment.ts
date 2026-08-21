/**
 * otelEnvironment: the OpenTelemetry environment variables, read the way the
 * specification says to read them.
 *
 * T3 Code has always had its own `T3CODE_OTLP_*` names, which stay the
 * explicit answer when they are set. Everything here is the fallback for the
 * far more common case: a machine that already exports `OTEL_*` for every
 * other service on it and expects one more process to join in without being
 * told twice.
 *
 * Read by every T3 Code process that exports telemetry, so the server and the
 * desktop app cannot disagree about what a variable means.
 *
 * Only the variables T3 Code can act on are read. The exporter speaks
 * OTLP over HTTP, so `grpc` is declined loudly rather than answered with a
 * body the endpoint cannot parse.
 *
 * Everything else the specification requires of an unusable value is a
 * warning followed by the default, never a refusal to start and never a
 * silently different behavior.
 *
 * @module otelEnvironment
 */
import * as Config from "effect/Config";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";

/**
 * The signals T3 Code exports. Each one is configured independently, and
 * the specification spells every variable name with the signal in it, so the
 * name is the thing the readers below are parameterized by.
 */
export type OtlpSignalName = "TRACES" | "METRICS" | "LOGS";

/** The wire formats T3 Code can produce. `grpc` is not one of them. */
export type OtlpProtocol = "http/json" | "http/protobuf";

/** `OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE`. */
export type MetricsTemporality = "cumulative" | "delta";

/** Everything one signal's exporter needs, or `undefined` if it is off. */
export interface OtlpSignalSettings {
  readonly url: string;
  /**
   * The wire format for this signal alone. Each signal builds its own
   * serializer, so the two are free to differ, and keeping the choice on the
   * signal is what stops it from reaching an endpoint these variables did not
   * supply.
   */
  readonly protocol: OtlpProtocol;
  readonly headers: Readonly<Record<string, string>> | undefined;
  readonly exportIntervalMs: number | undefined;
  readonly maxBatchSize: number | undefined;
  /** Metrics only. Spans and log records have no aggregation to prefer. */
  readonly temporality: MetricsTemporality | undefined;
}

/**
 * One signal's whole answer from these variables: how to export it, or why it
 * is not exported from here. Everything a signal decides lives under it, so a
 * caller whose endpoint came from somewhere else drops this one value and
 * leaves nothing behind that could reach an export it did not configure.
 */
export interface OtlpSignal {
  readonly settings: OtlpSignalSettings | undefined;
  /**
   * Why a configured endpoint is not being used, if it is not. Carried rather
   * than logged here so the caller can report it once, at startup, where a
   * user is looking.
   */
  readonly declined: string | undefined;
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
  readonly traces: OtlpSignal;
  readonly metrics: OtlpSignal;
  readonly logs: OtlpSignal;
  readonly resource: OtlpResourceSettings;
}

/**
 * An empty value means the same thing as an unset one. The specification says
 * so, and it is how a machine clears a variable it inherited without being
 * able to unset it. Surrounding whitespace is dropped for the same reason a
 * blank value is: a shell profile that padded a line did not mean the padding
 * to become part of an endpoint or a service name.
 */
const optionalString = (name: string) =>
  Config.string(name).pipe(
    Config.option,
    Config.map((value) => {
      const raw = Option.getOrUndefined(value)?.trim();
      return raw === undefined || raw === "" ? undefined : raw;
    }),
  );

/**
 * The specification defines exactly one true value: the case-insensitive
 * string `true`. Everything else is false, including values that read as
 * affirmative elsewhere, because implementations are told not to extend the
 * list.
 */
const specBoolean = (name: string) =>
  optionalString(name).pipe(Effect.map((raw) => raw?.toLowerCase() === "true"));

/**
 * A number that is not a number is warned about and dropped, which is what the
 * specification asks for anywhere a value is unrecognized. Letting the read
 * fail instead would take every other variable down with it and turn one typo
 * into no telemetry at all.
 */
const readInt = (name: string, warnings: Array<string>) =>
  optionalString(name).pipe(
    Effect.map((raw) => {
      if (raw === undefined) {
        return undefined;
      }
      const value = Number(raw);
      if (!Number.isSafeInteger(value) || value < 0) {
        warnings.push(`${name}=${raw} is not a whole number and was ignored`);
        return undefined;
      }
      return value;
    }),
  );

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
  // A value that produced no pair at all is a malformed list, not a request
  // for no headers. Returning `{}` here would count as a supplied value and
  // silently shadow the generic variable the signal should have fallen back
  // to.
  return Object.keys(entries).length === 0 ? undefined : entries;
};

interface Parsed<A> {
  readonly value: A | undefined;
  readonly warnings: ReadonlyArray<string>;
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
        return { value: undefined, warnings: [] };
      }
      const parsed = parseBaggage(raw);
      return parsed === undefined
        ? {
            value: undefined,
            warnings: [`${name} is not a valid list of key=value pairs and was ignored`],
          }
        : { value: parsed, warnings: [] };
    }),
  );

/**
 * `OTEL_EXPORTER_OTLP_<SIGNAL>_ENDPOINT` is a full URL and is used as given.
 * The generic `OTEL_EXPORTER_OTLP_ENDPOINT` is a base, and the spec has each
 * signal append its own path to it.
 */
const signalEndpoint = (signal: OtlpSignalName) =>
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
const signalWantsOtlp = (signal: OtlpSignalName) =>
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
const SPEC_DEFAULT_PROTOCOL = "http/protobuf" as const;
const SPEC_DEFAULT_MAX_EXPORT_BATCH_SIZE = 512;

/**
 * How often each signal drains, and how much it drains at once. The
 * specification gives every signal its own variables and its own defaults:
 * spans batch on `OTEL_BSP_*` every 5s, log records batch on `OTEL_BLRP_*`
 * every 1s, and metrics have no batch size because a collection cycle already
 * bounds itself.
 */
const SIGNAL_BATCHING = {
  TRACES: {
    scheduleDelay: "OTEL_BSP_SCHEDULE_DELAY",
    defaultScheduleDelayMs: 5_000,
    maxExportBatchSize: "OTEL_BSP_MAX_EXPORT_BATCH_SIZE",
  },
  METRICS: {
    scheduleDelay: "OTEL_METRIC_EXPORT_INTERVAL",
    defaultScheduleDelayMs: 60_000,
    maxExportBatchSize: undefined,
  },
  LOGS: {
    scheduleDelay: "OTEL_BLRP_SCHEDULE_DELAY",
    defaultScheduleDelayMs: 1_000,
    maxExportBatchSize: "OTEL_BLRP_MAX_EXPORT_BATCH_SIZE",
  },
} as const satisfies Record<
  OtlpSignalName,
  {
    readonly scheduleDelay: string;
    readonly defaultScheduleDelayMs: number;
    readonly maxExportBatchSize: string | undefined;
  }
>;

const signalSettings = (
  signal: OtlpSignalName,
  protocol: OtlpProtocol,
  temporality: MetricsTemporality | undefined,
) =>
  Effect.gen(function* () {
    const url = yield* signalEndpoint(signal);
    if (url === undefined || !(yield* signalWantsOtlp(signal))) {
      return { value: undefined, warnings: [] };
    }
    const numbers: Array<string> = [];
    const specific = yield* optionalRecord(`OTEL_EXPORTER_OTLP_${signal}_HEADERS`);
    const generic = yield* optionalRecord("OTEL_EXPORTER_OTLP_HEADERS");
    const headers = specific.value ?? generic.value;
    const batching = SIGNAL_BATCHING[signal];
    const exportIntervalMs =
      (yield* readInt(batching.scheduleDelay, numbers)) ?? batching.defaultScheduleDelayMs;
    const maxBatchSize =
      batching.maxExportBatchSize === undefined
        ? undefined
        : ((yield* readInt(batching.maxExportBatchSize, numbers)) ??
          SPEC_DEFAULT_MAX_EXPORT_BATCH_SIZE);
    return {
      value: {
        url,
        protocol,
        headers,
        exportIntervalMs,
        maxBatchSize,
        temporality: signal === "METRICS" ? temporality : undefined,
      },
      warnings: [...specific.warnings, ...generic.warnings, ...numbers],
    } satisfies Parsed<OtlpSignalSettings>;
  });

/** What one signal should do about its wire format. */
interface SignalProtocol {
  readonly protocol: OtlpProtocol;
  readonly declined: string | undefined;
}

interface ProtocolDecision {
  readonly traces: SignalProtocol;
  readonly metrics: SignalProtocol;
  readonly logs: SignalProtocol;
  readonly warnings: ReadonlyArray<string>;
}

/**
 * Left unset when nothing named a protocol, so a machine that never mentioned
 * OpenTelemetry keeps the wire format T3 Code has always used.
 *
 * `grpc` is the one value that turns export off rather than falling back. It
 * is a real protocol T3 Code does not speak, its endpoint has no
 * `/v1/traces` path, and it expects a framing nothing here produces, so
 * posting to it is worse than exporting nothing. It turns off only the signal
 * that named it, since a metric endpoint speaking gRPC says nothing about
 * where traces go. A value that is not a protocol at all is a typo, and the
 * specification is explicit that those get a warning and the default.
 *
 * Each signal builds its own serializer, so all three are answered separately
 * and are free to disagree.
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
      return { value, name } as const;
    }
    warnings.push(`${name}=${raw} is not a known OTLP protocol and was ignored`);
    return undefined;
  };

  const generic = yield* read("OTEL_EXPORTER_OTLP_PROTOCOL");
  const traces = (yield* read("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL")) ?? generic;
  const metrics = (yield* read("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL")) ?? generic;
  const logs = (yield* read("OTEL_EXPORTER_OTLP_LOGS_PROTOCOL")) ?? generic;

  const decide = (named: typeof generic): SignalProtocol =>
    named === undefined
      ? { protocol: SPEC_DEFAULT_PROTOCOL, declined: undefined }
      : named.value === "grpc"
        ? {
            protocol: SPEC_DEFAULT_PROTOCOL,
            declined: `${named.name}=grpc is not supported; T3 Code exports OTLP over HTTP only, so this signal is not exported`,
          }
        : { protocol: named.value, declined: undefined };

  return {
    traces: decide(traces),
    metrics: decide(metrics),
    logs: decide(logs),
    warnings,
  } satisfies ProtocolDecision;
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
      return { value: undefined, warnings: [] };
    }
    const preference = raw.trim().toLowerCase();
    if (preference === "delta" || preference === "cumulative") {
      return { value: preference, warnings: [] };
    }
    return {
      value: undefined,
      warnings: [
        preference === "lowmemory"
          ? "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE=lowmemory is not supported here; cumulative is used"
          : `OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE=${raw} is not a known preference and was ignored`,
      ],
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
    warnings: parsed.warnings,
  } satisfies Parsed<OtlpResourceSettings>;
});

const UNREADABLE = "the OpenTelemetry environment could not be read";

/**
 * Read the environment. Never fails: a variable T3 Code cannot honor
 * leaves the corresponding setting unset and is reported through the signal's
 * `declined`, because an unparseable telemetry knob is not a reason to refuse
 * to start.
 */
export const load: Effect.Effect<OtelEnvironment> = Effect.gen(function* () {
  const disabled = yield* specBoolean("OTEL_SDK_DISABLED");
  const protocolDecision = yield* resolveProtocol;
  const resource = yield* resolveResource;
  const temporality = yield* resolveMetricsTemporality;
  const traces = disabled
    ? { value: undefined, warnings: [] }
    : yield* signalSettings("TRACES", protocolDecision.traces.protocol, undefined);
  const metrics = disabled
    ? { value: undefined, warnings: [] }
    : yield* signalSettings("METRICS", protocolDecision.metrics.protocol, temporality.value);
  const logs = disabled
    ? { value: undefined, warnings: [] }
    : yield* signalSettings("LOGS", protocolDecision.logs.protocol, undefined);
  return {
    disabled,
    // Every signal reads the generic `OTEL_EXPORTER_OTLP_*` variables, so one
    // bad value arrives here once per signal and would be logged that often.
    warnings: [
      ...new Set([
        ...protocolDecision.warnings,
        ...resource.warnings,
        ...temporality.warnings,
        ...traces.warnings,
        ...metrics.warnings,
        ...logs.warnings,
      ]),
    ],
    // `value` is set only for a signal that resolved an endpoint and asked for
    // OTLP, so it is also the test for whether a decline is worth reporting. A
    // signal nothing pointed anywhere, one switched off by name, and every
    // signal once the SDK is disabled were never going to export, and saying
    // gRPC is why would name the wrong cause.
    traces: {
      settings: protocolDecision.traces.declined === undefined ? traces.value : undefined,
      declined: traces.value === undefined ? undefined : protocolDecision.traces.declined,
    },
    metrics: {
      settings: protocolDecision.metrics.declined === undefined ? metrics.value : undefined,
      declined: metrics.value === undefined ? undefined : protocolDecision.metrics.declined,
    },
    logs: {
      settings: protocolDecision.logs.declined === undefined ? logs.value : undefined,
      declined: logs.value === undefined ? undefined : protocolDecision.logs.declined,
    },
    resource: resource.value,
  };
}).pipe(
  Effect.catchCause((cause) =>
    Effect.logWarning("Could not read the OpenTelemetry environment", cause).pipe(
      Effect.as({
        disabled: false,
        warnings: [],
        traces: { settings: undefined, declined: UNREADABLE },
        metrics: { settings: undefined, declined: UNREADABLE },
        logs: { settings: undefined, declined: UNREADABLE },
        resource: { serviceName: undefined, serviceVersion: undefined, attributes: {} },
      }),
    ),
  ),
);

/** A signal these variables said nothing usable about. */
export const noSignal: OtlpSignal = { settings: undefined, declined: undefined };

/** An environment that asked for nothing, for tests and for the pairing CLI. */
export const none: OtelEnvironment = {
  disabled: false,
  warnings: [],
  traces: noSignal,
  metrics: noSignal,
  logs: noSignal,
  resource: { serviceName: undefined, serviceVersion: undefined, attributes: {} },
};
