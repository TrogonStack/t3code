import { describe, expect, it } from "@effect/vitest";
import { ProviderInstanceEnvironment } from "@t3tools/contracts";
import * as Schema from "effect/Schema";

import { hasProviderSecretReference, providerSecretReference } from "./ProviderSecretReference.ts";

const decodeEnvironment = Schema.decodeSync(ProviderInstanceEnvironment);

describe("providerSecretReference", () => {
  it("reads a 1Password reference", () => {
    expect(providerSecretReference("op://Private/claude-code/credential")).toBe(
      "op://Private/claude-code/credential",
    );
  });

  it("trims a reference pasted with surrounding whitespace", () => {
    expect(providerSecretReference("  op://Private/claude-code/credential\n")).toBe(
      "op://Private/claude-code/credential",
    );
  });

  it("treats a literal value as a literal", () => {
    expect(providerSecretReference("sk-live-token")).toBeUndefined();
    expect(providerSecretReference("/home/u/.claude/work")).toBeUndefined();
  });

  it("ignores a bare scheme with nothing behind it", () => {
    expect(providerSecretReference("op://")).toBeUndefined();
  });
});

describe("hasProviderSecretReference", () => {
  it("is false for an absent or empty environment", () => {
    expect(hasProviderSecretReference(undefined)).toBe(false);
    expect(hasProviderSecretReference([])).toBe(false);
  });

  it("is true when any single variable reads from the store", () => {
    expect(
      hasProviderSecretReference(
        decodeEnvironment([
          { name: "CLAUDE_SECURESTORAGE_CONFIG_DIR", value: "/home/u/.claude/work" },
          { name: "CLAUDE_CODE_OAUTH_TOKEN", value: "op://Private/claude-code/credential" },
        ]),
      ),
    ).toBe(true);
  });

  it("is false when every variable is a literal", () => {
    expect(
      hasProviderSecretReference(
        decodeEnvironment([
          { name: "CLAUDE_SECURESTORAGE_CONFIG_DIR", value: "/home/u/.claude/work" },
        ]),
      ),
    ).toBe(false);
  });
});
