import { httpHeaderRedactionLayer } from "@t3tools/shared/httpObservability";
import {
  makeLocalFileTracer,
  makeTraceSink,
  otlpSerializationLayer,
} from "@t3tools/shared/observability";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as References from "effect/References";
import * as Tracer from "effect/Tracer";
import * as OtlpExporter from "effect/unstable/observability/OtlpExporter";
import * as OtlpMetrics from "effect/unstable/observability/OtlpMetrics";
import * as OtlpTracer from "effect/unstable/observability/OtlpTracer";

import * as ServerConfig from "../../config.ts";
import * as ResourceAttribution from "../../resourceTelemetry/ResourceAttribution.ts";
import { ServerLoggerLive } from "../../serverLogger.ts";
import * as BrowserTraceCollector from "../BrowserTraceCollector.ts";

export const ObservabilityLive = Layer.unwrap(
  Effect.gen(function* () {
    const config = yield* ServerConfig.ServerConfig;
    const attribution = yield* ResourceAttribution.ResourceAttribution;
    const otel = config.otelEnvironment;

    for (const warning of otel.warnings) {
      yield* Effect.logWarning(warning);
    }

    if (otel.disabled) {
      yield* Effect.logWarning(
        "OTEL_SDK_DISABLED is set, so no telemetry is exported; this overrides T3CODE_OTLP_* and Settings too",
      );
    }

    // One variable can decline every signal, and saying so three times reads
    // like three separate problems.
    const declined = new Set(
      [otel.traces.declined, otel.metrics.declined, otel.logs.declined].filter(
        (reason) => reason !== undefined,
      ),
    );
    for (const reason of declined) {
      yield* Effect.logWarning(reason);
    }

    // Each signal builds its own serializer, so the wire format travels with
    // the settings of the endpoint that asked for it. A signal these variables
    // did not supply keeps what T3CODE_OTLP_PROTOCOL asked for.
    const serializationFor = (settings: typeof otel.traces.settings) =>
      otlpSerializationLayer(settings?.protocol ?? config.otlpProtocol);
    const headersFor = (settings: typeof otel.traces.settings) =>
      settings?.headers ?? config.otlpHeaders;

    const otlpResource = ServerConfig.otlpResource(config);

    const traceReferencesLayer = Layer.mergeAll(
      Layer.succeed(Tracer.MinimumTraceLevel, config.traceMinLevel),
      Layer.succeed(References.TracerTimingEnabled, config.traceTimingEnabled),
      httpHeaderRedactionLayer,
    );

    const tracerLayer = Layer.unwrap(
      Effect.gen(function* () {
        const sink = yield* makeTraceSink({
          filePath: config.serverTracePath,
          maxBytes: config.traceMaxBytes,
          maxFiles: config.traceMaxFiles,
          batchWindowMs: config.traceBatchWindowMs,
          onFlush: (stats) =>
            attribution.record({
              component: "server-trace",
              operation: "append",
              logicalWriteBytes: stats.logicalWriteBytes,
              count: stats.count,
              durationMs: stats.durationMs,
            }),
        });
        const delegate =
          config.otlpTracesUrl === undefined
            ? undefined
            : yield* OtlpTracer.make({
                url: config.otlpTracesUrl,
                exportInterval: `${config.otlpExportIntervalMs} millis`,
                resource: otlpResource,
                headers: headersFor(otel.traces.settings),
                ...(otel.traces.settings?.maxBatchSize === undefined
                  ? {}
                  : { maxBatchSize: otel.traces.settings.maxBatchSize }),
              });

        const tracer = yield* makeLocalFileTracer({
          filePath: config.serverTracePath,
          maxBytes: config.traceMaxBytes,
          maxFiles: config.traceMaxFiles,
          batchWindowMs: config.traceBatchWindowMs,
          sink,
          ...(delegate ? { delegate } : {}),
        });

        return Layer.mergeAll(
          Layer.succeed(Tracer.Tracer, tracer),
          BrowserTraceCollector.layer(sink),
        );
      }),
    ).pipe(
      Layer.provide(OtlpExporter.layerFlusher),
      // The trace serializer is also the one this layer hands out, because the
      // proxy in http.ts re-encodes browser spans and has to reach the trace
      // collector in the format that collector was configured for.
      Layer.provideMerge(serializationFor(otel.traces.settings)),
    );

    const metricsLayer =
      config.otlpMetricsUrl === undefined
        ? Layer.empty
        : OtlpMetrics.layer({
            url: config.otlpMetricsUrl,
            exportInterval: `${config.otlpMetricsExportIntervalMs} millis`,
            resource: otlpResource,
            headers: headersFor(otel.metrics.settings),
            ...(otel.metrics.settings?.temporality === undefined
              ? {}
              : { temporality: otel.metrics.settings.temporality }),
          }).pipe(Layer.provide(serializationFor(otel.metrics.settings)));

    return Layer.mergeAll(ServerLoggerLive, traceReferencesLayer, tracerLayer, metricsLayer);
  }),
);
