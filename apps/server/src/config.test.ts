import * as OtelEnvironment from "@t3tools/shared/otelEnvironment";
import { assert, describe, it } from "@effect/vitest";

import * as ServerConfig from "./config.ts";

describe("otlpResource", () => {
  const resource = (
    resourceAttributes: Record<string, string>,
    overrides: Partial<Pick<ServerConfig.ServerConfig["Service"], "mode" | "devUrl">> = {},
  ) =>
    ServerConfig.otlpResource(
      {
        mode: "web",
        devUrl: undefined,
        otelEnvironment: { ...OtelEnvironment.none, resourceAttributes },
        ...overrides,
      },
      "box-1",
    );

  it("reports who manages the server and its deployment environment", () => {
    const standalone: Readonly<Record<string, unknown>> = resource({}).attributes;
    assert.equal(standalone["t3code.server.managed_by"], "standalone");
    assert.equal(standalone["deployment.environment.name"], "production");
    assert.isFalse("service.runtime" in standalone);
    assert.isFalse("service.mode" in standalone);

    const desktopDev: Readonly<Record<string, unknown>> = resource(
      {},
      { mode: "desktop", devUrl: new URL("http://localhost:5733") },
    ).attributes;
    assert.equal(desktopDev["t3code.server.managed_by"], "desktop");
    assert.equal(desktopDev["deployment.environment.name"], "development");
  });
});
