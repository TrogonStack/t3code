import { describe, expect, it } from "vite-plus/test";
import * as Schema from "effect/Schema";

import { OnePasswordSecretSource } from "./onePassword.ts";

const decodeSource = Schema.decodeUnknownSync(OnePasswordSecretSource);

describe("OnePasswordSecretSource", () => {
  const onePassword = (reference: string, account = "my.1password.com") => ({
    kind: "1password",
    reference,
    account,
  });

  it.each([
    "op://Private/claude-code/credential",
    "op://Home Lab/Claude Code/API Key",
    "op://Private/claude-code/login section/password",
    "op://Private/github/one-time password?attribute=otp",
  ])("accepts the reference %s", (reference) => {
    expect(decodeSource(onePassword(reference))).toEqual(onePassword(reference));
  });

  it.each([
    ["leading whitespace", " op://Private/item/field"],
    ["trailing newline", "op://Private/item/field\n"],
    ["control character", "op://Private/it\u0007em/field"],
    ["missing scheme", "Private/item/field"],
    ["nothing after the scheme", "op://"],
    ["template delimiter", "op://Private/item}}/field"],
  ])("rejects a reference with %s", (_label, reference) => {
    expect(() => decodeSource(onePassword(reference))).toThrow();
  });

  it.each(["my", "my.1password.com", "team-acme.1password.eu", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"])(
    "accepts the account %s",
    (account) => {
      expect(decodeSource(onePassword("op://Private/item/field", account))).toMatchObject({
        account,
      });
    },
  );

  it.each([
    ["empty", ""],
    ["a space", "my account"],
    ["a leading dash", "--help"],
    ["surrounding whitespace", " my "],
  ])("rejects an account with %s", (_label, account) => {
    expect(() => decodeSource(onePassword("op://Private/item/field", account))).toThrow();
  });

  it("rejects a source without an account", () => {
    expect(() =>
      decodeSource({ kind: "1password", reference: "op://Private/item/field" }),
    ).toThrow();
  });
});
