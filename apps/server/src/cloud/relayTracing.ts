import {
  nodeProcessRuntimeAttributes,
  processServiceInstanceId,
} from "@t3tools/shared/observability";
import { makeRelayClientTracingLayer } from "@t3tools/shared/relayTracing";

import { resolveRelayClientTracingConfig } from "./publicConfig.ts";

const relayClientTracingConfig = resolveRelayClientTracingConfig();

export const headlessRelayClientTracingLayer = makeRelayClientTracingLayer(
  relayClientTracingConfig,
  {
    serviceName: "t3code-server",
    serviceInstanceId: processServiceInstanceId(),
    attributes: nodeProcessRuntimeAttributes(),
    client: "headless-cli",
  },
);

export const serverRelayBrokerTracingLayer = makeRelayClientTracingLayer(relayClientTracingConfig, {
  serviceName: "t3code-server",
  serviceInstanceId: processServiceInstanceId(),
  attributes: nodeProcessRuntimeAttributes(),
  client: "environment-server",
  component: "relay-broker",
});
