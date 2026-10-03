import Constants from "expo-constants";
import { makeRelayClientTracingLayer } from "@t3tools/shared/relayTracing";

import { hasTracingPublicConfig, resolveCloudPublicConfig } from "../cloud/publicConfig";

export interface TracingConfig {
  readonly tracesUrl: string;
  readonly tracesDataset: string;
  readonly tracesToken: string;
}

export interface TracingResource {
  readonly serviceVersion?: string;
  readonly serviceInstanceId: string;
  readonly appVariant: string;
}

export function resolveTracingConfig(): TracingConfig | null {
  const config = resolveCloudPublicConfig();
  if (!hasTracingPublicConfig(config)) {
    return null;
  }
  const { tracesUrl, tracesDataset, tracesToken } = config.observability;
  return { tracesUrl, tracesDataset, tracesToken };
}

export function makeTracingLayer(config: TracingConfig | null, resource: TracingResource) {
  return makeRelayClientTracingLayer(config, {
    serviceName: "t3code-mobile",
    serviceVersion: resource.serviceVersion,
    serviceInstanceId: resource.serviceInstanceId,
    client: "mobile",
    attributes: {
      "process.runtime.name": "react-native",
      ...(resource.appVariant !== "unknown" && {
        "deployment.environment.name": resource.appVariant,
      }),
    },
  });
}

export const tracingLayer = makeTracingLayer(resolveTracingConfig(), {
  serviceVersion: Constants.expoConfig?.version,
  serviceInstanceId: Constants.sessionId,
  appVariant:
    typeof Constants.expoConfig?.extra?.appVariant === "string"
      ? Constants.expoConfig.extra.appVariant
      : "unknown",
});
