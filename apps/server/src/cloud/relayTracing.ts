import {
  nodeProcessRuntimeAttributes,
  processServiceInstanceId,
} from "@t3tools/shared/observability";
import * as RelayTracing from "@t3tools/shared/relayTracing";
import { releaseResourceAttributes } from "@t3tools/shared/releaseResource";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import packageJson from "../../package.json" with { type: "json" };
import * as ServerConfig from "../config.ts";
import { resolveRelayClientTracingConfig } from "./publicConfig.ts";

const relayClientTracingConfig = resolveRelayClientTracingConfig();

const relayLayer = (
  resource: Omit<RelayTracing.RelayClientTracingResource, "serviceInstanceId" | "serviceVersion">,
) =>
  Layer.unwrap(
    Effect.map(ServerConfig.ServerConfig, (config) => {
      const operatorAttributes = config.otelEnvironment.resourceAttributes;
      return RelayTracing.layer(relayClientTracingConfig, {
        ...resource,
        serviceVersion: packageJson.version,
        serviceInstanceId: processServiceInstanceId(operatorAttributes),
        attributes: {
          ...resource.attributes,
          ...releaseResourceAttributes(ServerConfig.telemetryChannel, operatorAttributes),
        },
      });
    }),
  );

export const layerHeadlessRelayClient = relayLayer({
  serviceName: "t3code-server",
  runtime: "node",
  attributes: nodeProcessRuntimeAttributes(),
  client: "headless-cli",
});

export const layerServerRelayBroker = relayLayer({
  serviceName: "t3code-server",
  runtime: "node",
  attributes: nodeProcessRuntimeAttributes(),
  client: "environment-server",
  component: "relay-broker",
});
