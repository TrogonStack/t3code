import * as Schema from "effect/Schema";
import { assert, describe, expect, it } from "@effect/vitest";

import { definition, OpenBaoSecretProviderConfig } from "./definition.ts";

const decodeConfig = Schema.decodeSync(OpenBaoSecretProviderConfig);
const decodeUnknownConfig = Schema.decodeUnknownExit(OpenBaoSecretProviderConfig);

describe("OpenBao definition", () => {
  it("accepts path#field references", () => {
    for (const reference of ["agents/codex#api_key", "a#b"]) {
      expect(definition.isReference(reference), reference).toBe(true);
    }
  });

  it("rejects references without a path or field, flag-shaped paths, and control characters", () => {
    for (const reference of ["agents/codex", "#key", "agents/codex#", "-x#key", " a#b", "a#b\n"]) {
      expect(definition.isReference(reference), reference).toBe(false);
    }
  });

  it("defaults the mount and auth", () => {
    assert.deepStrictEqual(decodeConfig({ address: "https://bao.example" }), {
      address: "https://bao.example",
      mount: "secret",
      auth: "token",
    });
  });

  it("rejects a non-http address and a flag-shaped mount", () => {
    const decode = decodeUnknownConfig;
    expect(decode({ address: "ftp://bao" })._tag).toBe("Failure");
    expect(decode({ address: "https://bao.example", mount: "-x" })._tag).toBe("Failure");
  });
});
