import { httpHeaderRedactionLayer } from "@t3tools/shared/httpObservability";
import {
  makeLocalFileTracer,
  makeTraceSink,
  otlpSerializationLayer,
  type SignalExport,
} from "@t3tools/shared/observability";
import * as OtelEnvironment from "@t3tools/shared/otelEnvironment";
import * as ConfigProvider from "effect/ConfigProvider";
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

    // One variable can decline every signal, and saying so three times reads
    // like three separate problems.
    const declined = new Set(
      [otel.traces.declined, otel.metrics.declined, otel.logs.declined].filter(
        (reason) => reason !== undefined,
      ),
    );

    // Each signal builds its own serializer, so the wire format travels with
    // the endpoint that asked for it rather than with this process.
    const serializationFor = (signal: SignalExport) => otlpSerializationLayer(signal.protocol);

    const otlpResource = ServerConfig.otlpResource(config);

    // Every exporter builds its resource through `OtlpResource.fromConfig`,
    // which reads `OTEL_RESOURCE_ATTRIBUTES` for itself and turns a value it
    // cannot percent-decode into a defect, so a list this reader reported and
    // dropped would still stop the server from starting. The exporters are
    // shown the list it validated instead. Only that one name is answered here
    // and every other variable still comes from the environment.
    const resourceAttributesLayer = ConfigProvider.layerAdd(
      ConfigProvider.fromEnv({
        env: {
          OTEL_RESOURCE_ATTRIBUTES: OtelEnvironment.encodeResourceAttributes(
            otel.resource.attributes,
          ),
        },
        preserveEmptyStrings: true,
      }),
      { asPrimary: true },
    );

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

    // Logged once the server's loggers are installed, so the warnings use them.
    const otelWarningsLayer = Layer.effectDiscard(
      Effect.forEach([...otel.warnings, ...declined], (warning) => Effect.logWarning(warning)),
    );

    return otelWarningsLayer.pipe(
      Layer.provideMerge(
        Layer.mergeAll(ServerLoggerLive, traceReferencesLayer, tracerLayer, metricsLayer),
      ),
      Layer.provide(resourceAttributesLayer),
    );
  }),
);
