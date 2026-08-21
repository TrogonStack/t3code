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

/** The wire format a `T3CODE_OTLP_*` or Settings endpoint has always been sent. */
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

/** An endpoint named outside the OpenTelemetry variables, per signal. */
export interface DesktopNamedOtlpEndpoints {
  readonly traces: string | undefined;
  readonly metrics: string | undefined;
  readonly logs: string | undefined;
}

export interface DesktopOtlpExportInput {
  readonly otel: OtelEnvironment.OtelEnvironment;
  readonly named: DesktopNamedOtlpEndpoints;
  /** `T3CODE_OTLP_EXPORT_INTERVAL_MS`, which deliberately covers every signal. */
  readonly namedExportIntervalMs: number | undefined;
  /** Used when nothing named a service, so existing dashboards keep working. */
  readonly defaultServiceName: string;
  /**
   * What this process is, as opposed to what the machine calls the service.
   * Applied last so an ambient `OTEL_RESOURCE_ATTRIBUTES` cannot make the main
   * process claim to be the server.
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
 * A signal whose endpoint came from somewhere else is not the OpenTelemetry
 * variables' to configure. Dropping the whole signal, rather than the endpoint
 * alone, is what stops an ambient `OTEL_EXPORTER_OTLP_ENDPOINT` from changing
 * the wire format, headers, or batching of an export a `T3CODE_OTLP_*` name or
 * Settings already answered.
 */
const resolveSignal = (
  named: string | undefined,
  signal: OtelEnvironment.OtlpSignal,
  namedExportIntervalMs: number | undefined,
): DesktopOtlpSignal => {
  const settings = named === undefined ? signal.settings : undefined;
  const url = named ?? settings?.url;
  if (url === undefined) {
    return offSignal;
  }
  return {
    url,
    exportIntervalMs:
      namedExportIntervalMs ?? settings?.exportIntervalMs ?? DEFAULT_DESKTOP_EXPORT_INTERVAL_MS,
    protocol: settings?.protocol ?? DEFAULT_DESKTOP_PROTOCOL,
    headers: settings?.headers,
    maxBatchSize: settings?.maxBatchSize,
    temporality: settings?.temporality,
  };
};

export const resolveDesktopOtlpExport = (input: DesktopOtlpExportInput): DesktopOtlpExport => {
  const { otel, named } = input;
  const resource: DesktopOtlpResource = {
    serviceName: otel.resource.serviceName ?? input.defaultServiceName,
    serviceVersion: otel.resource.serviceVersion,
    attributes: { ...otel.resource.attributes, ...input.runtimeAttributes },
  };

  if (otel.disabled) {
    return {
      traces: offSignal,
      metrics: offSignal,
      logs: offSignal,
      resource,
      warnings: [
        ...otel.warnings,
        "OTEL_SDK_DISABLED is set, so the desktop app exports no telemetry; this overrides T3CODE_OTLP_* and Settings too",
      ],
    };
  }

  const signals = {
    traces: named.traces === undefined ? otel.traces : OtelEnvironment.noSignal,
    metrics: named.metrics === undefined ? otel.metrics : OtelEnvironment.noSignal,
    logs: named.logs === undefined ? otel.logs : OtelEnvironment.noSignal,
  };

  // One variable can decline every signal, and saying so three times reads
  // like three separate problems.
  return {
    traces: resolveSignal(named.traces, signals.traces, input.namedExportIntervalMs),
    metrics: resolveSignal(named.metrics, signals.metrics, input.namedExportIntervalMs),
    logs: resolveSignal(named.logs, signals.logs, input.namedExportIntervalMs),
    resource,
    warnings: [
      ...new Set([
        ...otel.warnings,
        ...[signals.traces.declined, signals.metrics.declined, signals.logs.declined].filter(
          (reason): reason is string => reason !== undefined,
        ),
      ]),
    ],
  };
};
