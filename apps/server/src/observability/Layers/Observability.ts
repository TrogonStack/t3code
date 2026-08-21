import { httpHeaderRedactionLayer } from "@t3tools/shared/httpObservability";
import { makeLocalFileTracer, makeTraceSink } from "@t3tools/shared/observability";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as References from "effect/References";
import * as Tracer from "effect/Tracer";
import * as OtlpExporter from "effect/unstable/observability/OtlpExporter";
import * as OtlpMetrics from "effect/unstable/observability/OtlpMetrics";
import * as OtlpSerialization from "effect/unstable/observability/OtlpSerialization";
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

    if (otel.declined !== undefined) {
      yield* Effect.logWarning(otel.declined);
    }

    const configuredThroughOtelEnvironment =
      otel.traces !== undefined || otel.metrics !== undefined;
    const protocol =
      otel.protocol ?? (configuredThroughOtelEnvironment ? "http/protobuf" : "http/json");
    const otlpSerializationLayer =
      protocol === "http/protobuf" ? OtlpSerialization.layerProtobuf : OtlpSerialization.layerJson;

    const otlpResource = {
      serviceName: config.otlpServiceName,
      ...(otel.resource.serviceVersion === undefined
        ? {}
        : { serviceVersion: otel.resource.serviceVersion }),
      attributes: {
        ...otel.resource.attributes,
        "service.runtime": "t3-server",
        "service.mode": config.mode,
      },
    };

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
                ...(otel.traces?.headers === undefined ? {} : { headers: otel.traces.headers }),
                ...(otel.traces?.maxBatchSize === undefined
                  ? {}
                  : { maxBatchSize: otel.traces.maxBatchSize }),
                ...(otel.traces?.shutdownTimeoutMs === undefined
                  ? {}
                  : { shutdownTimeout: `${otel.traces.shutdownTimeoutMs} millis` as const }),
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
    ).pipe(Layer.provide(OtlpExporter.layerFlusher), Layer.provideMerge(otlpSerializationLayer));

    const metricsLayer =
      config.otlpMetricsUrl === undefined
        ? Layer.empty
        : OtlpMetrics.layer({
            url: config.otlpMetricsUrl,
            exportInterval: `${otel.metrics?.exportIntervalMs ?? config.otlpExportIntervalMs} millis`,
            resource: otlpResource,
            ...(otel.metrics?.headers === undefined ? {} : { headers: otel.metrics.headers }),
            ...(otel.metrics?.shutdownTimeoutMs === undefined
              ? {}
              : { shutdownTimeout: `${otel.metrics.shutdownTimeoutMs} millis` as const }),
            ...(otel.metricsTemporality === undefined
              ? {}
              : { temporality: otel.metricsTemporality }),
          }).pipe(Layer.provideMerge(otlpSerializationLayer));

    return Layer.mergeAll(ServerLoggerLive, traceReferencesLayer, tracerLayer, metricsLayer);
  }),
);
