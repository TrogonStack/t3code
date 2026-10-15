import { describe, expect, it } from "vite-plus/test";
import * as Schema from "effect/Schema";

import { OpenBaoSecretProviderConfig, SecretProviderConfig } from "./secretProvider.ts";
import { secretSettingsFields } from "./settingsSecret.ts";
import { ProviderInstanceEnvironmentVariable } from "./providerInstance.ts";

const decodeConfig = Schema.decodeUnknownSync(SecretProviderConfig);
const decodeVariable = Schema.decodeUnknownSync(ProviderInstanceEnvironmentVariable);

describe("SecretProviderConfig", () => {
  it("decodes a 1Password provider", () => {
    const config = { kind: "1password", label: "Personal", account: "my.1password.com" };
    expect(decodeConfig(config)).toEqual(config);
  });

  it("defaults the OpenBao mount and keeps the optional fields", () => {
    expect(
      decodeConfig({ kind: "openbao", label: " Homelab ", address: "https://bao.example:8200" }),
    ).toEqual({
      kind: "openbao",
      label: "Homelab",
      address: "https://bao.example:8200",
      mount: "secret",
      access: "api",
    });
    const full = {
      kind: "openbao",
      label: "Homelab",
      address: "http://127.0.0.1:8200",
      mount: "kv/team",
      namespace: "admin",
      token: "s.token",
      access: "cli",
    };
    expect(decodeConfig(full)).toEqual(full);
  });

  it("marks only the OpenBao token as a stored secret", () => {
    expect(secretSettingsFields(OpenBaoSecretProviderConfig)).toEqual(["token"]);
  });

  it.each([
    ["an empty label", { label: "" }],
    ["a long label", { label: "x".repeat(65) }],
    ["a non-http address", { address: "file:///etc/passwd" }],
    ["an unparsable address", { address: "bao.example" }],
    ["a mount starting with a dash", { mount: "-secret" }],
    ["a mount with a space", { mount: "my secret" }],
    ["an empty namespace", { namespace: "" }],
  ])("rejects an OpenBao provider with %s", (_label, override) => {
    expect(() =>
      decodeConfig({
        kind: "openbao",
        label: "Homelab",
        address: "https://bao.example:8200",
        ...override,
      }),
    ).toThrow();
  });

  it("rejects a 1Password provider without an account", () => {
    expect(() => decodeConfig({ kind: "1password", label: "Personal" })).toThrow();
  });
});

describe("SecretProviderSecretSource", () => {
  const source = (reference: string) => ({
    kind: "secret-provider",
    provider: "homelab",
    reference,
  });

  it.each(["op://Private/item/field", "apps/claude#api_key", "team/app config#token"])(
    "decodes the reference %s as an environment value",
    (reference) => {
      expect(decodeVariable({ name: "API_KEY", value: source(reference) }).value).toEqual(
        source(reference),
      );
    },
  );

  it.each([
    ["empty", ""],
    ["leading whitespace", " apps/claude#key"],
    ["a trailing newline", "apps/claude#key\n"],
    ["a control character", "apps/cl\u0007aude#key"],
    ["a template delimiter", "apps/{{claude}}#key"],
    ["too many characters", "x".repeat(1025)],
  ])("rejects a reference with %s", (_label, reference) => {
    expect(() => decodeVariable({ name: "API_KEY", value: source(reference) })).toThrow();
  });

  it("still decodes the legacy 1Password source", () => {
    const value = { kind: "1password", reference: "op://Private/item/field", account: "my" };
    expect(decodeVariable({ name: "API_KEY", value }).value).toEqual(value);
  });
});
