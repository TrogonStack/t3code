import type { ManagedPolicySummary } from "@t3tools/contracts";
import * as HostProcess from "@t3tools/shared/HostProcess";
import {
  nodeProcessRuntimeAttributes,
  processServiceInstanceId,
} from "@t3tools/shared/observability";
import { isRelayTracingEnabled } from "@t3tools/shared/otelEnvironment";
import * as RelayTracing from "@t3tools/shared/relayTracing";
import { releaseResourceAttributes } from "@t3tools/shared/releaseResource";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import packageJson from "../../package.json" with { type: "json" };
import * as ServerConfig from "../config.ts";
import * as ManagedPolicy from "../policy/ManagedPolicy.ts";
import { resolveRelayClientTracingConfig } from "./publicConfig.ts";

const relayClientTracingConfig = resolveRelayClientTracingConfig();

/**
 * `false` once the environment or a managed policy opted out. Until the desktop has read the
 * login shell, a shell-profile opt-out is still unknown, so tracing stays off.
 */
export const allowedFor = (
  env: Readonly<Record<string, string | undefined>>,
  policy: ManagedPolicySummary | undefined,
) =>
  policy?.limits?.telemetry !== false &&
  env.T3CODE_DESKTOP_SHELL_DISCOVERY_PENDING !== "1" &&
  isRelayTracingEnabled(env);

/** Relay traces leave the machine for T3 Connect, so a policy with `telemetry: false` stops them. */
export const relayLayer = (
  resource: Omit<RelayTracing.RelayClientTracingResource, "serviceInstanceId" | "serviceVersion">,
  tracingConfig = relayClientTracingConfig,
) =>
  Layer.unwrap(
    Effect.gen(function* () {
      const policy = yield* ManagedPolicy.ManagedPolicy;
      // Desktop shell discovery updates this record in place, so it is reread per span.
      const environment = yield* HostProcess.Environment;
      return RelayTracing.layer(
        ManagedPolicy.deniedBy(policy, "telemetry") === undefined ? tracingConfig : null,
        // Never the operator's own resource attributes: they describe their machine for their receiver.
        {
          ...resource,
          serviceVersion: packageJson.version,
          serviceInstanceId: processServiceInstanceId(),
          attributes: {
            ...resource.attributes,
            ...releaseResourceAttributes(ServerConfig.telemetryChannel),
          },
        },
        { isEnabled: () => allowedFor(environment, undefined) },
      );
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
