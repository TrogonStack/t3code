/**
 * What the Electron main process exports, and where.
 *
 * The main process is its own OpenTelemetry producer: it owns app startup,
 * window and menu work, backend supervision, and updates, none of which the
 * server process can see. It reads the same sources as the server and in the
 * same order, so a machine that points one of them at a collector points both.
 *
 * @module app/DesktopOtlpExport
 */
import * as OtelEnvironment from "@t3tools/shared/otelEnvironment";

/**
 * The interval T3 Code has always used for a `T3CODE_OTLP_*` or Settings
 * endpoint. An `OTEL_*` endpoint brings the specification's own per-signal
 * default instead.
 */
export const DEFAULT_DESKTOP_EXPORT_INTERVAL_MS = 10_000;

/**
 * The wire format a `T3CODE_OTLP_*` or Settings endpoint is sent when
 * `T3CODE_OTLP_PROTOCOL` does not name one.
 */
const DEFAULT_DESKTOP_PROTOCOL: OtelEnvironment.OtlpProtocol = "http/json";

export interface DesktopOtlpSignal {
  readonly url: string | undefined;
  readonly exportIntervalMs: number;
  readonly protocol: OtelEnvironment.OtlpProtocol;
  readonly headers: Readonly<Record<string, string>> | undefined;
  readonly maxBatchSize: number | undefined;
  readonly temporality: OtelEnvironment.MetricsTemporality | undefined;
}

export interface DesktopOtlpResource {
  readonly serviceName: string;
  readonly serviceVersion: string | undefined;
  readonly attributes: Readonly<Record<string, string>>;
}

export interface DesktopOtlpExport {
  readonly traces: DesktopOtlpSignal;
  readonly metrics: DesktopOtlpSignal;
  readonly logs: DesktopOtlpSignal;
  readonly resource: DesktopOtlpResource;
  /** Everything worth saying out loud once, already phrased for a human. */
  readonly warnings: ReadonlyArray<string>;
}

/** One endpoint per signal, from a single source. */
export interface DesktopOtlpEndpoints {
  readonly traces: string | undefined;
  readonly metrics: string | undefined;
  readonly logs: string | undefined;
}

export interface DesktopOtlpExportInput {
  readonly otel: OtelEnvironment.OtelEnvironment;
  /** `T3CODE_OTLP_*`, which outranks everything. */
  readonly named: DesktopOtlpEndpoints;
  /** Settings, which answers under both sets of variables. */
  readonly persisted: DesktopOtlpEndpoints;
  /** `T3CODE_OTLP_EXPORT_INTERVAL_MS`, which deliberately covers every signal. */
  readonly namedExportIntervalMs: number | undefined;
  /** `T3CODE_OTLP_HEADERS`, which deliberately covers every signal. */
  readonly namedHeaders: Readonly<Record<string, string>> | undefined;
  /** `T3CODE_OTLP_PROTOCOL`, which deliberately covers every signal. */
  readonly namedProtocol: OtelEnvironment.OtlpProtocol | undefined;
  /** What this process calls itself. The environment cannot rename it. */
  readonly serviceName: string;
  /**
   * What this process is. Applied last so an ambient
   * `OTEL_RESOURCE_ATTRIBUTES` cannot make the main process claim to be the
   * server.
   */
  readonly runtimeAttributes: Readonly<Record<string, string>>;
}

const offSignal: DesktopOtlpSignal = {
  url: undefined,
  exportIntervalMs: DEFAULT_DESKTOP_EXPORT_INTERVAL_MS,
  protocol: DEFAULT_DESKTOP_PROTOCOL,
  headers: undefined,
  maxBatchSize: undefined,
  temporality: undefined,
};

/**
 * Turns the source that won one signal into what the exporter needs.
 * `OtelEnvironment.resolveSignalSource` has already decided which source that
 * is, and hands back settings only when the `OTEL_*` variables are the ones
 * that named the endpoint.
 */
const resolveSignal = (
  resolved: { readonly url: string | undefined; readonly signal: OtelEnvironment.OtlpSignal },
  input: DesktopOtlpExportInput,
): DesktopOtlpSignal => {
  const settings = resolved.signal.settings;
  if (resolved.url === undefined) {
    return offSignal;
  }
  return {
    url: resolved.url,
    exportIntervalMs:
      input.namedExportIntervalMs ??
      settings?.exportIntervalMs ??
      DEFAULT_DESKTOP_EXPORT_INTERVAL_MS,
    protocol: settings?.protocol ?? input.namedProtocol ?? DEFAULT_DESKTOP_PROTOCOL,
    headers: settings?.headers ?? input.namedHeaders,
    maxBatchSize: settings?.maxBatchSize,
    temporality: settings?.temporality,
  };
};

export const resolveDesktopOtlpExport = (input: DesktopOtlpExportInput): DesktopOtlpExport => {
  const { otel } = input;
  const resource: DesktopOtlpResource = {
    serviceName: input.serviceName,
    serviceVersion: otel.resource.serviceVersion,
    attributes: { ...otel.resource.attributes, ...input.runtimeAttributes },
  };

  if (otel.disabled) {
    return {
      traces: offSignal,
      metrics: offSignal,
      logs: offSignal,
      resource,
      warnings: [...otel.warnings],
    };
  }

  const traces = OtelEnvironment.resolveSignalSource({
    t3Url: input.named.traces,
    signal: otel.traces,
    persistedUrl: input.persisted.traces,
  });
  const metrics = OtelEnvironment.resolveSignalSource({
    t3Url: input.named.metrics,
    signal: otel.metrics,
    persistedUrl: input.persisted.metrics,
  });
  const logs = OtelEnvironment.resolveSignalSource({
    t3Url: input.named.logs,
    signal: otel.logs,
    persistedUrl: input.persisted.logs,
  });

  // One variable can decline every signal, and saying so three times reads
  // like three separate problems.
  return {
    traces: resolveSignal(traces, input),
    metrics: resolveSignal(metrics, input),
    logs: resolveSignal(logs, input),
    resource,
    warnings: [
      ...new Set([
        ...otel.warnings,
        ...[traces.signal.declined, metrics.signal.declined, logs.signal.declined].filter(
          (reason): reason is string => reason !== undefined,
        ),
      ]),
    ],
  };
};
