import { otlpSerializationLayer } from "@t3tools/shared/observability";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Logger from "effect/Logger";
import * as References from "effect/References";
import * as OtlpExporter from "effect/unstable/observability/OtlpExporter";
import * as OtlpLogger from "effect/unstable/observability/OtlpLogger";

import * as ServerConfig from "./config.ts";

/**
 * Every logger the server installs, built in one `Logger.layer` call because
 * that call writes the whole set at once. A second layer that also installs a
 * logger either replaces this set or merges with the one the fiber had before
 * either layer ran, depending on merge order, so the OTLP log exporter belongs
 * here next to the console and tracer loggers rather than beside them in the
 * observability layer.
 */
export const ServerLoggerLive = Effect.gen(function* () {
  const config = yield* ServerConfig.ServerConfig;
  const settings = config.otelEnvironment.logs.settings;
  // A log endpoint these variables did not supply keeps the headers and wire
  // format T3 Code's own names asked for.
  const headers = settings?.headers ?? config.otlpHeaders;
  const otlpLogger =
    config.otlpLogsUrl === undefined
      ? undefined
      : OtlpLogger.make({
          url: config.otlpLogsUrl,
          exportInterval: `${config.otlpLogsExportIntervalMs} millis`,
          resource: ServerConfig.otlpResource(config),
          ...(headers === undefined ? {} : { headers }),
          ...(settings?.maxBatchSize === undefined ? {} : { maxBatchSize: settings.maxBatchSize }),
        });

  const minimumLogLevelLayer = Layer.succeed(References.MinimumLogLevel, config.logLevel);
  const loggerLayer = Logger.layer(
    [
      Logger.consolePretty(),
      Logger.tracerLogger,
      ...(otlpLogger === undefined ? [] : [otlpLogger]),
    ],
    { mergeWithExisting: false },
  ).pipe(
    Layer.provide(OtlpExporter.layerFlusher),
    Layer.provide(otlpSerializationLayer(settings?.protocol ?? config.otlpProtocol)),
  );

  return Layer.mergeAll(loggerLayer, minimumLogLevelLayer);
}).pipe(Layer.unwrap);
