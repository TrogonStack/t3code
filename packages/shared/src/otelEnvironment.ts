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
 * desktop app cannot disagree about what a variable means. That is also why
 * `T3CODE_OTEL_SDK_DISABLED` is read here: it is the same setting as
 * `OTEL_SDK_DISABLED`, asked of T3 Code's own name first.
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

/**
 * What metrics are aggregated as when nothing asks for anything, which is the
 * specification's default and the one Prometheus and Mimir want. It is named
 * here, and applied at the exporter rather than left to the exporter's own
 * fallback, so the value that ships is decided in one place instead of tracking
 * whatever a dependency happens to default to.
 *
 * Sending it to a receiver that accepts delta histograms only, which is how
 * Datadog's OTLP intake behaves, loses every timer in silence while the
 * counters keep arriving. That is a backend fact rather than a bad default, so
 * the answer is to set the variable, not to invert this for everyone.
 */
export const DEFAULT_METRICS_TEMPORALITY: MetricsTemporality = "cumulative";

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

/**
 * What the environment may contribute to the resource. `service.name` is
 * deliberately absent: each T3 Code process names itself and nothing here can
 * rename it, so a fleet-wide `OTEL_SERVICE_NAME` cannot quietly merge two
 * processes into one service or file T3 Code under some other app's name. An
 * attempt to set it is reported through `warnings` rather than ignored in
 * silence.
 */
export interface OtlpResourceSettings {
  readonly serviceVersion: string | undefined;
  readonly attributes: Readonly<Record<string, string>>;
}

export interface OtelEnvironment {
  /**
   * Whether anything is exported at all, by any route. `T3CODE_OTEL_SDK_DISABLED`
   * answers it, and `OTEL_SDK_DISABLED` answers it only when T3 Code's own name
   * is unset, which is the source order every other setting here follows. So
   * `T3CODE_OTEL_SDK_DISABLED=false` is how a machine that exports
   * `OTEL_SDK_DISABLED` for everything else keeps T3 Code exporting.
   */
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
/**
 * A set but blank value is not an answer. Taking one as an answer publishes an
 * endpoint nothing can reach and suppresses the source under it that could
 * have been used instead.
 */
const blankAsUnset = (value: string | undefined) => {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed === "" ? undefined : trimmed;
};

const optionalString = (name: string) =>
  Config.String(name).pipe(
    Config.option,
    Config.map((value) => blankAsUnset(Option.getOrUndefined(value))),
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
 * A `T3CODE_*` name is ours, so it answers to the affirmatives people actually
 * type rather than the single value the specification allows. `undefined` means
 * the name did not answer, either because it is unset or because its value was
 * unreadable, and the source under it decides instead. A typo therefore costs
 * that variable and nothing else, the same as everywhere else here.
 */
const t3Boolean = (name: string) =>
  optionalString(name).pipe(
    Effect.map(
      (raw): { readonly value: boolean | undefined; readonly warnings: ReadonlyArray<string> } => {
        if (raw === undefined) {
          return { value: undefined, warnings: [] };
        }
        const value = raw.toLowerCase();
        if (["true", "1", "yes", "on"].includes(value)) {
          return { value: true, warnings: [] };
        }
        if (["false", "0", "no", "off"].includes(value)) {
          return { value: false, warnings: [] };
        }
        return {
          value: undefined,
          warnings: [`${name}=${raw} is not a yes or a no and was ignored`],
        };
      },
    ),
  );

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
 * cannot express, because one temporality is applied to every instrument here
 * rather than chosen per instrument kind. It resolves to `delta` instead of the
 * default, and says so.
 *
 * `delta` is the honest answer rather than a near-enough one. `lowmemory` asks
 * for delta on synchronous counters and histograms and cumulative on the rest,
 * and every metric T3 Code records is a monotonic counter or a timer, so the
 * kinds the two preferences disagree about are kinds nothing here produces.
 * Falling back to the default would have inverted the only part of the request
 * that is about the data, and inverted it toward the value that loses it: a
 * receiver that accepts delta histograms only, which is how Datadog's OTLP
 * intake behaves, drops cumulative histograms without reporting an error, so
 * every duration metric would disappear while the counters kept arriving.
 *
 * A value that is not a preference at all is a different case and stays
 * ignored. It carries no intent to honor.
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
    if (preference === "lowmemory") {
      return {
        value: "delta",
        warnings: [
          "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE=lowmemory cannot be expressed per instrument kind here, so delta is used for every metric, which is what lowmemory asks for on the counters and timers T3 Code records",
        ],
      };
    }
    return {
      value: undefined,
      warnings: [
        `OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE=${raw} is not a known preference and was ignored, so metrics are exported as ${DEFAULT_METRICS_TEMPORALITY}`,
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
  // Named here only to say it was refused. Dropping it without a word is the
  // failure this variable is prone to: the name never changes, the dashboards
  // stay empty, and nothing in the log connects the two.
  const declinedName =
    (yield* optionalString("OTEL_SERVICE_NAME")) === undefined
      ? attributeName === undefined
        ? undefined
        : "OTEL_RESOURCE_ATTRIBUTES=service.name"
      : "OTEL_SERVICE_NAME";
  return {
    value: {
      serviceVersion: (yield* optionalString("OTEL_SERVICE_VERSION")) ?? attributeVersion,
      attributes: rest,
    },
    warnings: [
      ...parsed.warnings,
      ...(declinedName === undefined
        ? []
        : [
            `${declinedName} was ignored; every T3 Code process names itself, so use OTEL_RESOURCE_ATTRIBUTES to tell instances apart instead`,
          ]),
    ],
  } satisfies Parsed<OtlpResourceSettings>;
});

const UNREADABLE = "the OpenTelemetry environment could not be read";

/**
 * Whichever name switched export off is the one worth naming, because it is
 * the one the reader has to go and unset. An ambient `OTEL_SDK_DISABLED` is
 * the case where that is not obvious and where the answer is not to unset
 * anything, so the message carries the override with it.
 */
const disabledBy = (name: string) =>
  name === "OTEL_SDK_DISABLED"
    ? "OTEL_SDK_DISABLED is set, so no telemetry is exported, whatever configured it; set T3CODE_OTEL_SDK_DISABLED=false to export anyway"
    : `${name} is set, so no telemetry is exported, whatever configured it`;

/**
 * Read the environment. Never fails: a variable T3 Code cannot honor
 * leaves the corresponding setting unset and is reported through the signal's
 * `declined`, because an unparseable telemetry knob is not a reason to refuse
 * to start.
 */
export const load: Effect.Effect<OtelEnvironment> = Effect.gen(function* () {
  const t3 = yield* t3Boolean("T3CODE_OTEL_SDK_DISABLED");
  const spec = yield* specBoolean("OTEL_SDK_DISABLED");
  // One setting, read the way every other setting here is read: T3 Code's own
  // name answers it, and the standard name answers it only when ours is unset.
  const disabled = t3.value ?? spec;
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
        ...t3.warnings,
        ...(disabled
          ? [disabledBy(t3.value === true ? "T3CODE_OTEL_SDK_DISABLED" : "OTEL_SDK_DISABLED")]
          : []),
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
        resource: { serviceVersion: undefined, attributes: {} },
      }),
    ),
  ),
);

/** A signal these variables said nothing usable about. */
const noSignal: OtlpSignal = { settings: undefined, declined: undefined };

/**
 * Where one signal's endpoint comes from, and therefore which source
 * configures the rest of it. Sources are asked in the order every setting
 * here follows: T3 Code's own name, then the standard `OTEL_*` names, then
 * whatever was persisted, meaning the desktop bootstrap envelope or Settings.
 * An exported variable outranks a stored one, and T3 Code's own spelling of a
 * variable outranks the standard spelling of it.
 *
 * Whichever source wins takes the whole signal and not the URL alone, so the
 * signal returned here is `noSignal` unless `OTEL_*` is what won. That is what
 * stops an ambient `OTEL_EXPORTER_OTLP_ENDPOINT` from changing the wire
 * format, headers, batching, or aggregation of an export it never pointed
 * anywhere, and stops startup reporting a signal as declined while it is
 * exporting. When nothing names an endpoint the signal is returned as it was
 * read, because a declined transport is still worth saying when there is no
 * export to confuse it with.
 *
 * Read by every process that exports, so the server and the desktop app
 * cannot resolve the same machine's variables differently.
 */
export const resolveSignalSource = (input: {
  readonly t3Url: string | undefined;
  readonly signal: OtlpSignal;
  readonly persistedUrl: string | undefined;
}): { readonly url: string | undefined; readonly signal: OtlpSignal } => {
  const t3Url = blankAsUnset(input.t3Url);
  if (t3Url !== undefined) {
    return { url: t3Url, signal: noSignal };
  }
  if (input.signal.settings !== undefined) {
    return { url: input.signal.settings.url, signal: input.signal };
  }
  const persistedUrl = blankAsUnset(input.persistedUrl);
  return persistedUrl === undefined
    ? { url: undefined, signal: input.signal }
    : { url: persistedUrl, signal: noSignal };
};

/** An environment that asked for nothing, for tests and for the pairing CLI. */
export const none: OtelEnvironment = {
  disabled: false,
  warnings: [],
  traces: noSignal,
  metrics: noSignal,
  logs: noSignal,
  resource: { serviceVersion: undefined, attributes: {} },
};
