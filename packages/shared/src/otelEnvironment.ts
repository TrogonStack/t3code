/**
 * otelEnvironment: the OpenTelemetry environment variables, read the way the
 * specification says to read them. Read by every T3 Code process that exports
 * telemetry, so the server and the desktop app cannot disagree about what a
 * variable means.
 *
 * Parsing is the Config algebra's. What this module adds on top of it, and the
 * only reasoning that crosses the functions below, is:
 *
 * - An unusable value is a warning and the default, never a refusal to start,
 *   which is `ignoring`.
 * - A signal's own variable owns that signal once it is set, which is `owned`.
 * - The exporter speaks OTLP over HTTP, so `grpc` is declined loudly.
 *
 * Why each of those is the right answer, the precedence between the sources,
 * and every variable this reads is in `docs/operations/observability.md`.
 *
 * @module otelEnvironment
 */
import * as Config from "effect/Config";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";
import * as SchemaIssue from "effect/SchemaIssue";
import * as SchemaTransformation from "effect/SchemaTransformation";

import {
  DEFAULT_METRICS_TEMPORALITY,
  type MetricsTemporality,
  type OtlpProtocol,
  type SignalExport,
} from "./observability.ts";

/** The signals T3 Code exports, spelled as the variable names spell them. */
export type OtlpSignalName = "TRACES" | "METRICS" | "LOGS";

/** Everything one signal's exporter needs, or `undefined` if it is off. */
export interface OtlpSignalSettings {
  readonly url: string;
  /** Per signal, since each signal builds its own serializer. */
  readonly protocol: OtlpProtocol;
  readonly headers: Readonly<Record<string, string>> | undefined;
  readonly exportIntervalMs: number | undefined;
  readonly maxBatchSize: number | undefined;
  /** Metrics only. Spans and log records have no aggregation to prefer. */
  readonly temporality: MetricsTemporality | undefined;
}

/**
 * One signal's whole answer from these variables, so a caller whose endpoint
 * came from somewhere else drops this one value and leaves nothing behind.
 */
export interface OtlpSignal {
  readonly settings: OtlpSignalSettings | undefined;
  /** Why a configured endpoint is not used, for the caller to report at startup. */
  readonly declined: string | undefined;
  /**
   * Whether these variables named this signal and then asked for no export. Kept
   * apart from absent `settings`, which is what lets a stored endpoint answer.
   */
  readonly off: boolean;
}

/**
 * What the environment may contribute to the resource. `service.name` is
 * deliberately absent: each T3 Code process names itself, and an attempt to set
 * it is reported through `warnings`.
 */
export interface OtlpResourceSettings {
  readonly serviceVersion: string | undefined;
  readonly attributes: Readonly<Record<string, string>>;
}

export interface OtelEnvironment {
  /** Whether anything is exported at all. */
  readonly disabled: boolean;
  /** Settings that were named but could not be used, phrased for a human. */
  readonly warnings: ReadonlyArray<string>;
  readonly traces: OtlpSignal;
  readonly metrics: OtlpSignal;
  readonly logs: OtlpSignal;
  readonly resource: OtlpResourceSettings;
}

/** A set but blank value reads as unset, so the source under it can answer. */
export const blankAsUnset = (value: string | undefined) => {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed === "" ? undefined : trimmed;
};

/** A value this reader could use, and what to say about the ones it could not. */
interface Parsed<A> {
  readonly value: A | undefined;
  readonly warnings: ReadonlyArray<string>;
}

const usable = <A>(value: A | undefined): Parsed<A> => ({ value, warnings: [] });

/**
 * Turns a read that would fail into the default plus a warning naming the value
 * that caused it. Wrap every leaf read below in this. Pass `secret` for a
 * variable whose value can carry a credential, and the warning names the
 * variable without quoting what was in it: these warnings are written to the
 * startup log, and a header list or a URL with userinfo in it would put the
 * token there in plain text.
 */
const ignoring = <A>(
  name: string,
  expected: string,
  config: Config.Config<A>,
  options?: { readonly secret?: boolean },
): Config.Config<Parsed<A>> =>
  config.pipe(
    Config.option,
    Config.map((value) => usable(Option.getOrUndefined(value))),
    Config.orElse(() =>
      Config.String(name).pipe(
        Config.map((raw): Parsed<A> => ({
          value: undefined,
          warnings: [
            options?.secret === true
              ? `${name} is not ${expected} and was ignored`
              : `${name}=${raw} is not ${expected} and was ignored`,
          ],
        })),
      ),
    ),
  );

/**
 * Picks between a signal's own variable and the generic one. The signal's own
 * owns it the moment it is set, unusable values included.
 */
const owned = <A>(
  names: { readonly own: string; readonly generic: string },
  own: Parsed<A>,
  generic: Parsed<A>,
): Parsed<A> =>
  own.value === undefined && own.warnings.length === 0
    ? generic
    : {
        value: own.value,
        warnings: [
          ...own.warnings,
          ...generic.warnings,
          ...(own.value === undefined && generic.value !== undefined
            ? [`${names.own} names this signal, so ${names.generic} was not used in its place`]
            : []),
        ],
      };

/** Tidies a raw value before a schema reads it. */
const cleaned = <S extends Schema.Codec<any, string>>(
  clean: (value: string) => string,
  schema: S,
) =>
  Schema.String.pipe(
    Schema.decodeTo(
      schema,
      SchemaTransformation.transform({ decode: clean, encode: (value: string) => value }),
    ),
  );

const folded = <S extends Schema.Codec<any, string>>(schema: S) =>
  cleaned((value) => value.trim().toLowerCase(), schema);

/** One side of a pair list, as the exporters decode it. */
const PairComponent = cleaned((value) => value.trim(), Schema.StringFromUriComponent);

const optionalString = (name: string) =>
  Config.String(name).pipe(
    Config.option,
    Config.map((value) => blankAsUnset(Option.getOrUndefined(value))),
  );

/**
 * An `OTEL_*` boolean, which the specification spells `true` or `false` and
 * nothing else. Anything else is reported rather than quietly answered.
 */
const specBoolean = (name: string) =>
  ignoring(
    name,
    "true or false",
    Config.schema(folded(Schema.Literals(["true", "false"])), name),
  ).pipe(Config.map((parsed) => ({ value: parsed.value === "true", warnings: parsed.warnings })));

/**
 * A `T3CODE_*` boolean, which is ours and takes the affirmatives people type.
 * `undefined` leaves the source under it to answer.
 */
const AFFIRMATIVE: ReadonlySet<string> = new Set(["true", "1", "yes", "on"]);

const t3Boolean = (name: string) =>
  ignoring(
    name,
    "a yes or a no",
    Config.schema(
      folded(Schema.Literals(["true", "1", "yes", "on", "false", "0", "no", "off"])),
      name,
    ),
  ).pipe(
    Config.map((parsed) => ({
      value: parsed.value === undefined ? undefined : AFFIRMATIVE.has(parsed.value),
      warnings: parsed.warnings,
    })),
  );

/** Intervals and batch sizes, where zero is a busy loop rather than a number. */
const positiveInt = (name: string, subject: string) =>
  ignoring(
    name,
    `${subject} above zero`,
    Config.schema(Schema.Int.check(Schema.isGreaterThan(0)), name),
  );

/**
 * Headers are a W3C Baggage string, read more strictly here than `Config.Record`
 * reads one: its splitter divides on every `=`, which truncates base64 basic
 * auth at its padding, and keeps the readable members of a malformed list, which
 * is how `authorization=token,x-tenant` authenticates and then routes to the
 * wrong tenant.
 */
const headerPairs = (raw: string): Readonly<Record<string, string>> | undefined => {
  const entries: Record<string, string> = {};
  for (const member of raw.split(",")) {
    // Trailing and doubled commas are whitespace, not a member.
    if (member.trim() === "") {
      continue;
    }
    const separator = member.indexOf("=");
    if (separator === -1) {
      return undefined;
    }
    const key = member.slice(0, separator).trim();
    if (key === "") {
      return undefined;
    }
    try {
      entries[decodeURIComponent(key)] = decodeURIComponent(member.slice(separator + 1).trim());
    } catch {
      return undefined;
    }
  }
  // No pair at all is malformed rather than a request for no headers.
  return Object.keys(entries).length === 0 ? undefined : entries;
};

const HeaderList = Schema.String.pipe(
  Schema.decodeTo(
    Schema.Record(Schema.String, Schema.String),
    SchemaTransformation.transformEffect<Readonly<Record<string, string>>, string>({
      decode: (raw, options) => {
        const pairs = headerPairs(raw);
        return pairs === undefined
          ? Effect.fail(
              new SchemaIssue.InvalidValue({ expected: "a list of key=value pairs" }, raw, options),
            )
          : Effect.succeed(pairs);
      },
      encode: (pairs) => Effect.succeed(encodeResourceAttributes(pairs)),
    }),
  ),
);

const headerList = (name: string) =>
  ignoring(name, "a valid list of key=value pairs", Config.schema(HeaderList, name), {
    secret: true,
  });

const RESOURCE_ATTRIBUTES = "OTEL_RESOURCE_ATTRIBUTES";

/**
 * The write side of the same format, for shadowing the variable with the
 * validated list before an exporter reads it for itself.
 */
export const encodeResourceAttributes = (attributes: Readonly<Record<string, string>>) =>
  Object.entries(attributes)
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`)
    .join(",");

/**
 * `OTEL_EXPORTER_OTLP_<SIGNAL>_ENDPOINT` is a full URL and is used as given. The
 * generic `OTEL_EXPORTER_OTLP_ENDPOINT` is a base each signal appends its own
 * path to, on the path rather than on the end of the string, since the base may
 * carry the query an intake takes its API key in.
 */
const signalEndpoint = (signal: OtlpSignalName) => {
  const names = {
    own: `OTEL_EXPORTER_OTLP_${signal}_ENDPOINT`,
    generic: "OTEL_EXPORTER_OTLP_ENDPOINT",
  };
  return Effect.gen(function* () {
    const own = yield* ignoring(names.own, "a URL", Config.URL(names.own), { secret: true });
    const generic = yield* ignoring(
      names.generic,
      "a URL",
      Config.URL(names.generic).pipe(
        Config.map((base) => {
          const url = new URL(base);
          url.pathname = `${url.pathname.replace(/\/+$/, "")}/v1/${signal.toLowerCase()}`;
          return url;
        }),
      ),
      { secret: true },
    );
    const endpoint = owned(names, own, generic);
    return { value: endpoint.value?.toString(), warnings: endpoint.warnings };
  });
};

/**
 * The exporters the specification names for each signal that T3 Code has no
 * implementation of. Naming one is a deliberate "not OTLP". Per signal rather
 * than pooled, so `OTEL_LOGS_EXPORTER=prometheus` reads as the mistake it is.
 */
const FOREIGN_EXPORTERS: Readonly<Record<OtlpSignalName, ReadonlySet<string>>> = {
  TRACES: new Set(["console", "logging", "zipkin", "jaeger"]),
  METRICS: new Set(["console", "logging", "prometheus"]),
  LOGS: new Set(["console", "logging"]),
};

/**
 * `OTEL_<SIGNAL>_EXPORTER` is a list, and `otlp` is its default. A list naming
 * other exporters and not `otlp` is a deliberate "not this one"; a list naming
 * nothing recognizable is a typo and leaves the default in place.
 */
const signalWantsOtlp = (signal: OtlpSignalName) => {
  const name = `OTEL_${signal}_EXPORTER`;
  return Effect.gen(function* () {
    const raw = yield* optionalString(name);
    if (raw === undefined) {
      return usable(true);
    }
    const entries = yield* Config.Array(Schema.String, name).pipe(
      Config.map((list) =>
        list.map((entry) => entry.trim().toLowerCase()).filter((entry) => entry !== ""),
      ),
    );
    if (entries.includes("otlp")) {
      return {
        value: true,
        warnings: entries.some((entry) => entry !== "otlp")
          ? [
              `${name}=${raw} names otlp, so this signal is exported over OTLP and nothing else in that list is honored`,
            ]
          : [],
      };
    }
    if (!entries.some((entry) => entry === "none" || FOREIGN_EXPORTERS[signal].has(entry))) {
      return {
        value: true,
        warnings: [
          `${name}=${raw} names no exporter T3 Code recognizes and was ignored, so this signal is still exported over OTLP`,
        ],
      };
    }
    // `none` is the specification's own way to say "export nothing" and needs no
    // explanation. A foreign exporter does.
    return {
      value: false,
      warnings: entries.includes("none")
        ? []
        : [
            `${name}=${raw} asks for an exporter T3 Code does not have, so this signal is not exported`,
          ],
    };
  });
};

/** The specification's own defaults, which apply once this route configures the exporter. */
const SPEC_DEFAULT_PROTOCOL = "http/protobuf" as const;
const SPEC_DEFAULT_MAX_EXPORT_BATCH_SIZE = 512;

/**
 * How often each signal drains and how much it drains at once, under the
 * variables and defaults the specification gives that signal. Metrics have no
 * batch size because a collection cycle already bounds itself.
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

/** One signal as these variables read it, before the transport decision. */
interface ReadSignal extends Parsed<OtlpSignalSettings> {
  readonly off: boolean;
}

const signalSettings = (
  signal: OtlpSignalName,
  protocol: OtlpProtocol,
  /** Metrics only, and passed in so an aggregation shares its endpoint's fate. */
  temporality: Parsed<MetricsTemporality> | undefined,
) =>
  Effect.gen(function* () {
    const endpoint = yield* signalEndpoint(signal);
    // Nothing else about a signal is read or reported until it has somewhere to go.
    if (endpoint.value === undefined) {
      return { value: undefined, warnings: endpoint.warnings, off: false } satisfies ReadSignal;
    }
    const wantsOtlp = yield* signalWantsOtlp(signal);
    if (!wantsOtlp.value) {
      return { value: undefined, warnings: wantsOtlp.warnings, off: true } satisfies ReadSignal;
    }
    const names = {
      own: `OTEL_EXPORTER_OTLP_${signal}_HEADERS`,
      generic: "OTEL_EXPORTER_OTLP_HEADERS",
    };
    const headers = owned(names, yield* headerList(names.own), yield* headerList(names.generic));
    const batching = SIGNAL_BATCHING[signal];
    const interval = yield* positiveInt(batching.scheduleDelay, "an export interval");
    const batchSize =
      batching.maxExportBatchSize === undefined
        ? usable<number>(undefined)
        : yield* positiveInt(batching.maxExportBatchSize, "a batch size");
    return {
      value: {
        url: endpoint.value,
        protocol,
        headers: headers.value,
        exportIntervalMs: interval.value ?? batching.defaultScheduleDelayMs,
        maxBatchSize:
          batching.maxExportBatchSize === undefined
            ? undefined
            : (batchSize.value ?? SPEC_DEFAULT_MAX_EXPORT_BATCH_SIZE),
        temporality: temporality?.value,
      },
      warnings: [
        ...wantsOtlp.warnings,
        ...headers.warnings,
        ...interval.warnings,
        ...batchSize.warnings,
        ...(temporality?.warnings ?? []),
      ],
      off: false,
    } satisfies ReadSignal;
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

const wireProtocol = (name: string) =>
  ignoring(
    name,
    "a known OTLP protocol",
    Config.schema(folded(Schema.Literals(["http/json", "http/protobuf", "grpc"])), name),
  ).pipe(Config.map((parsed) => ({ ...parsed, name })));

/**
 * Each signal builds its own serializer, so all three are answered separately
 * and are free to disagree, and `grpc` declines only the signal that named it.
 */
const resolveProtocol = Effect.gen(function* () {
  const generic = yield* wireProtocol("OTEL_EXPORTER_OTLP_PROTOCOL");
  const named = {
    traces: yield* wireProtocol("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL"),
    metrics: yield* wireProtocol("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL"),
    logs: yield* wireProtocol("OTEL_EXPORTER_OTLP_LOGS_PROTOCOL"),
  };
  const decide = (own: typeof generic): SignalProtocol => {
    const asked = own.value === undefined ? generic : own;
    if (asked.value === undefined) {
      return { protocol: SPEC_DEFAULT_PROTOCOL, declined: undefined };
    }
    return asked.value === "grpc"
      ? {
          protocol: SPEC_DEFAULT_PROTOCOL,
          declined: `${asked.name}=grpc is not supported; T3 Code exports OTLP over HTTP only, so this signal is not exported`,
        }
      : { protocol: asked.value, declined: undefined };
  };
  return {
    traces: decide(named.traces),
    metrics: decide(named.metrics),
    logs: decide(named.logs),
    warnings: [
      ...generic.warnings,
      ...named.traces.warnings,
      ...named.metrics.warnings,
      ...named.logs.warnings,
    ],
  } satisfies ProtocolDecision;
});

const TEMPORALITY_PREFERENCE = "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE";

/**
 * `lowmemory` is a real preference this exporter cannot express per instrument
 * kind, and resolves to `delta` because that is what it asks for on the counters
 * and timers T3 Code records.
 */
const resolveMetricsTemporality = ignoring(
  TEMPORALITY_PREFERENCE,
  "a known preference",
  Config.schema(
    folded(Schema.Literals(["cumulative", "delta", "lowmemory"])),
    TEMPORALITY_PREFERENCE,
  ),
).pipe(
  Config.map((preference): Parsed<MetricsTemporality> =>
    preference.value === "lowmemory"
      ? {
          value: "delta",
          warnings: [
            `${TEMPORALITY_PREFERENCE}=lowmemory cannot be expressed per instrument kind here, so delta is used for every metric sent to the endpoint these variables named, which is what lowmemory asks for on the counters and timers T3 Code records`,
          ],
        }
      : {
          value: preference.value,
          warnings: preference.warnings.map(
            (warning) => `${warning}, so metrics are exported as ${DEFAULT_METRICS_TEMPORALITY}`,
          ),
        },
  ),
);

/** Attributes are read with the schema the exporters read the same variable with. */
const resolveResource = Effect.gen(function* () {
  const attributes = yield* ignoring(
    RESOURCE_ATTRIBUTES,
    "a valid list of key=value pairs",
    Config.Record(PairComponent, PairComponent, RESOURCE_ATTRIBUTES),
  );
  const {
    "service.name": attributeName,
    "service.version": attributeVersion,
    ...rest
  } = attributes.value ?? {};
  const serviceName = yield* optionalString("OTEL_SERVICE_NAME");
  // Read only to say it was refused, rather than dropped without a word.
  const declinedName =
    serviceName !== undefined
      ? "OTEL_SERVICE_NAME"
      : attributeName === undefined
        ? undefined
        : `${RESOURCE_ATTRIBUTES}=service.name`;
  return {
    value: {
      serviceVersion: (yield* optionalString("OTEL_SERVICE_VERSION")) ?? attributeVersion,
      attributes: rest,
    },
    warnings: [
      ...attributes.warnings,
      ...(declinedName === undefined
        ? []
        : [
            `${declinedName} was ignored; every T3 Code process names itself, so use OTEL_RESOURCE_ATTRIBUTES to tell instances apart instead`,
          ]),
    ],
  } satisfies Parsed<OtlpResourceSettings>;
});

const UNREADABLE = "the OpenTelemetry environment could not be read";

/** Names whichever variable switched export off, and how to overrule it. */
const disabledBy = (name: string) =>
  name === "OTEL_SDK_DISABLED"
    ? "OTEL_SDK_DISABLED is set, so no telemetry is exported, whatever configured it; set T3CODE_OTEL_SDK_DISABLED=false to export anyway"
    : `${name} is set, so no telemetry is exported, whatever configured it`;

/**
 * One signal's whole answer, with the transport decision folded in. A decline is
 * reported only for a signal that had somewhere to go and asked for OTLP, since
 * on any other signal gRPC would name the wrong cause.
 */
const signalOf = (signal: ReadSignal, transport: SignalProtocol): OtlpSignal =>
  signal.value === undefined || transport.declined === undefined
    ? { settings: signal.value, declined: undefined, off: signal.off }
    : { settings: undefined, declined: transport.declined, off: true };

/**
 * Read the environment. Never fails: a variable T3 Code cannot honor leaves its
 * setting unset and is reported through `warnings`.
 */
export const load: Effect.Effect<OtelEnvironment> = Effect.gen(function* () {
  const t3 = yield* t3Boolean("T3CODE_OTEL_SDK_DISABLED");
  const spec = yield* specBoolean("OTEL_SDK_DISABLED");
  const disabled = t3.value ?? spec.value;
  const protocolDecision = yield* resolveProtocol;
  const resource = yield* resolveResource;
  const temporality = yield* resolveMetricsTemporality;
  const silent: ReadSignal = { value: undefined, warnings: [], off: false };
  const traces = disabled
    ? silent
    : yield* signalSettings("TRACES", protocolDecision.traces.protocol, undefined);
  const metrics = disabled
    ? silent
    : yield* signalSettings("METRICS", protocolDecision.metrics.protocol, temporality);
  const logs = disabled
    ? silent
    : yield* signalSettings("LOGS", protocolDecision.logs.protocol, undefined);
  return {
    disabled,
    // Every signal reads the generic variables, so one bad value arrives thrice.
    warnings: [
      ...new Set([
        ...t3.warnings,
        ...spec.warnings,
        ...(disabled
          ? [disabledBy(t3.value === true ? "T3CODE_OTEL_SDK_DISABLED" : "OTEL_SDK_DISABLED")]
          : []),
        ...protocolDecision.warnings,
        ...resource.warnings,
        ...traces.warnings,
        ...metrics.warnings,
        ...logs.warnings,
      ]),
    ],
    traces: signalOf(traces, protocolDecision.traces),
    metrics: signalOf(metrics, protocolDecision.metrics),
    logs: signalOf(logs, protocolDecision.logs),
    resource: resource.value ?? { serviceVersion: undefined, attributes: {} },
  };
}).pipe(
  Effect.catchCause((cause) =>
    Effect.logWarning("Could not read the OpenTelemetry environment", cause).pipe(
      Effect.as({
        disabled: false,
        warnings: [],
        traces: { settings: undefined, declined: UNREADABLE, off: false },
        metrics: { settings: undefined, declined: UNREADABLE, off: false },
        logs: { settings: undefined, declined: UNREADABLE, off: false },
        resource: { serviceVersion: undefined, attributes: {} },
      }),
    ),
  ),
);

/** A signal these variables said nothing usable about. */
const noSignal: OtlpSignal = { settings: undefined, declined: undefined, off: false };

/**
 * Applies the whole-signal rule to the knobs and not only to the URL. Call this
 * rather than `settings?.headers ?? t3Headers`, which collapses the two cases it
 * has to keep apart: absent settings mean the standard variables named nothing,
 * while settings silent about one knob mean they own the signal and left that
 * knob unset, which is an answer of its own.
 */
export const resolveSignalExport = (input: {
  readonly settings: OtlpSignalSettings | undefined;
  readonly t3Protocol: OtlpProtocol;
  readonly t3Headers: Readonly<Record<string, string>> | undefined;
  readonly t3ExportIntervalMs: number;
}): SignalExport =>
  input.settings === undefined
    ? {
        protocol: input.t3Protocol,
        headers: input.t3Headers,
        exportIntervalMs: input.t3ExportIntervalMs,
        maxBatchSize: undefined,
        temporality: DEFAULT_METRICS_TEMPORALITY,
      }
    : {
        protocol: input.settings.protocol,
        headers: input.settings.headers,
        exportIntervalMs: input.settings.exportIntervalMs ?? input.t3ExportIntervalMs,
        maxBatchSize: input.settings.maxBatchSize,
        temporality: input.settings.temporality ?? DEFAULT_METRICS_TEMPORALITY,
      };

/**
 * Where one signal's endpoint comes from, and therefore which source configures
 * the rest of it: T3 Code's own name, then the standard `OTEL_*` names, then
 * whatever was persisted. Whichever source wins takes the whole signal, so the
 * signal returned is `noSignal` unless `OTEL_*` is what won, and a signal those
 * variables switched off is not one they said nothing about.
 *
 * Read by every process that exports, so two of them cannot resolve the same
 * machine's variables differently.
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
  if (input.signal.off) {
    return { url: undefined, signal: input.signal };
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
