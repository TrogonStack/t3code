import { describe, expect, it } from "vite-plus/test";

import { clientTracingResource } from "./clientResource";

const resource = (overrides: Partial<Parameters<typeof clientTracingResource>[0]> = {}) =>
  clientTracingResource({
    version: "1.2.3",
    isDevelopment: false,
    releaseChannel: undefined,
    surface: "web",
    serviceInstanceId: "instance-1",
    navigator: undefined,
    ...overrides,
  });

describe("clientTracingResource", () => {
  it.each([
    ["a dev build", { isDevelopment: true }, "development", "dev"],
    ["a stable build", {}, "production", "stable"],
    ["an unrecognized channel", { releaseChannel: "unrecognized" }, "production", "stable"],
    ["a nightly build", { version: "1.2.3-nightly.20261023.4" }, "production", "nightly"],
    ["a PR preview", { releaseChannel: "preview" }, "staging", "preview"],
    ["a preview version", { version: "1.2.3-preview.20261023.1" }, "staging", "preview"],
  ] as const)("reports %s as %s on the %s channel", (_name, overrides, environment, channel) => {
    const { attributes } = resource(overrides);
    expect(attributes["deployment.environment.name"]).toBe(environment);
    expect(attributes["t3code.release.channel"]).toBe(channel);
  });

  it("keeps the version untouched so the server version comparison still works", () => {
    expect(resource({ releaseChannel: "preview" }).serviceVersion).toBe("1.2.3");
  });

  it("names the desktop window and the browser as separate surfaces", () => {
    expect(resource({ surface: "desktop" }).attributes["t3code.client.surface"]).toBe("desktop");
    expect(resource().attributes["t3code.client.surface"]).toBe("web");
  });

  it("describes the browser when one is present", () => {
    const { attributes } = resource({
      navigator: { userAgent: "UA", language: "en-US" },
    });
    expect(attributes["user_agent.original"]).toBe("UA");
    expect(attributes["browser.language"]).toBe("en-US");
  });
});
