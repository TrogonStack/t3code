import { httpHeaderRedactionLayer } from "@t3tools/shared/httpObservability";
import * as OtelEnvironment from "@t3tools/shared/otelEnvironment";
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
    // the endpoint that asked for it rather than with this process.
    const serializationFor = (signal: OtelEnvironment.SignalExport) =>
      otlpSerializationLayer(signal.protocol);

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
                exportInterval: `${config.otlpTracesExport.exportIntervalMs} millis`,
                resource: otlpResource,
                headers: config.otlpTracesExport.headers,
                ...(config.otlpTracesExport.maxBatchSize === undefined
                  ? {}
                  : { maxBatchSize: config.otlpTracesExport.maxBatchSize }),
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
      Layer.provideMerge(serializationFor(config.otlpTracesExport)),
    );

    const metricsLayer =
      config.otlpMetricsUrl === undefined
        ? Layer.empty
        : OtlpMetrics.layer({
            url: config.otlpMetricsUrl,
            exportInterval: `${config.otlpMetricsExport.exportIntervalMs} millis`,
            resource: otlpResource,
            headers: config.otlpMetricsExport.headers,
            temporality: config.otlpMetricsExport.temporality,
          }).pipe(Layer.provide(serializationFor(config.otlpMetricsExport)));

    return Layer.mergeAll(ServerLoggerLive, traceReferencesLayer, tracerLayer, metricsLayer);
  }),
);
