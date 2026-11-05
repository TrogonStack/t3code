import * as Schema from "effect/Schema";
import { describe, expect, it } from "vite-plus/test";

import { ProviderInstanceEnvironmentVariable } from "./providerInstance.ts";

const decodeEnvironmentVariable = Schema.decodeUnknownSync(ProviderInstanceEnvironmentVariable);

describe("ProviderInstanceEnvironmentVariable literals", () => {
  it("decodes a string value as a literal, even one that looks like a reference", () => {
    expect(
      decodeEnvironmentVariable({ name: "API_KEY", value: "op://Private/item/field" }).value,
    ).toBe("op://Private/item/field");
  });

  it("treats a stored reference as sensitive even when the flag is missing or false", () => {
    const secret = { provider: "onepassword", reference: "op://Private/item/field" };
    expect(decodeEnvironmentVariable({ name: "API_KEY", secret })).toMatchObject({
      value: secret,
      sensitive: true,
    });
    expect(decodeEnvironmentVariable({ name: "API_KEY", sensitive: false, secret }).sensitive).toBe(
      true,
    );
  });
});
