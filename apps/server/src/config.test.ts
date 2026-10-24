import * as OtelEnvironment from "@t3tools/shared/otelEnvironment";
import { assert, describe, it } from "@effect/vitest";

import packageJson from "../package.json" with { type: "json" };
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

  it("reports the server's own version, release channel and environment", () => {
    const attributes: Readonly<Record<string, unknown>> = resource({}).attributes;
    assert.equal(resource({}).serviceVersion, packageJson.version);
    assert.equal(attributes["t3code.release.channel"], "stable");
    assert.equal(attributes["deployment.environment.name"], "production");

    const dev: Readonly<Record<string, unknown>> = resource(
      {},
      { devUrl: new URL("http://localhost:5733") },
    ).attributes;
    assert.equal(dev["t3code.release.channel"], "dev");

    const operator: Readonly<Record<string, unknown>> = resource({
      "t3code.release.channel": "nightly",
    }).attributes;
    assert.equal(operator["t3code.release.channel"], "nightly");
  });

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
