import { otlpSerializationLayer } from "@t3tools/shared/observability";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Logger from "effect/Logger";
import * as References from "effect/References";
import * as OtlpExporter from "effect/unstable/observability/OtlpExporter";
import * as OtlpLogger from "effect/unstable/observability/OtlpLogger";

import * as ServerConfig from "./config.ts";

export const ServerLoggerLive = Effect.gen(function* () {
  const config = yield* ServerConfig.ServerConfig;
  const minimumLogLevelLayer = Layer.succeed(References.MinimumLogLevel, config.logLevel);

  // `Logger.layer` replaces the whole logger set on every call, so a second
  // call installing the OTLP logger elsewhere would silently drop whichever
  // set lost the race instead of merging with it. The OTLP log exporter has
  // to join console/tracer loggers in this one call for both to survive.
  const otlpLogger =
    config.otlpLogsUrl === undefined
      ? undefined
      : OtlpLogger.make({
          url: config.otlpLogsUrl,
          exportInterval: `${config.otlpExportIntervalMs} millis`,
          headers: config.otlpHeaders,
          resource: ServerConfig.otlpResource(config),
        });

  const loggerLayer = Logger.layer(
    [
      Logger.consolePretty(),
      Logger.tracerLogger,
      ...(otlpLogger === undefined ? [] : [otlpLogger]),
    ],
    { mergeWithExisting: false },
  ).pipe(
    Layer.provide(OtlpExporter.layerFlusher),
    Layer.provide(otlpSerializationLayer(config.otlpProtocol)),
  );

  return Layer.mergeAll(loggerLayer, minimumLogLevelLayer);
}).pipe(Layer.unwrap);
