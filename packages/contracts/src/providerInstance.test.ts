import * as Schema from "effect/Schema";
import { describe, expect, it } from "vite-plus/test";

import { ProviderInstanceEnvironmentVariable } from "./providerInstance.ts";

const decodeEnvironmentVariable = Schema.decodeUnknownSync(ProviderInstanceEnvironmentVariable);

describe("ProviderInstanceEnvironmentVariable secret sources", () => {
  it("decodes a 1Password source", () => {
    const value = { kind: "1password", reference: "op://Private/item/field", account: "my" };
    expect(decodeEnvironmentVariable({ name: "API_KEY", value }).value).toEqual(value);
  });

  it("decodes a named secret provider source", () => {
    const value = { kind: "secret-provider", provider: "homelab", reference: "apps/claude#key" };
    expect(decodeEnvironmentVariable({ name: "API_KEY", value }).value).toEqual(value);
  });

  it("decodes a string value as a literal, even one that looks like a reference", () => {
    expect(
      decodeEnvironmentVariable({ name: "API_KEY", value: "op://Private/item/field" }).value,
    ).toBe("op://Private/item/field");
  });
});
