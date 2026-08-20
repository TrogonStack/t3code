import { describe, expect, it } from "@effect/vitest";
import { ProviderInstanceEnvironment } from "@t3tools/contracts";
import * as Schema from "effect/Schema";

import {
  collectProviderSecretReferences,
  hasProviderSecretReference,
  providerSecretReference,
} from "./ProviderSecretReference.ts";

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

describe("collectProviderSecretReferences", () => {
  it("returns each distinct reference once, in first-seen order", () => {
    const shared = "op://Private/shared/credential";
    const environments = [
      decodeEnvironment([
        { name: "CLAUDE_CODE_OAUTH_TOKEN", value: shared, sensitive: true },
        { name: "HOME", value: "/home/u" },
      ]),
      undefined,
      decodeEnvironment([
        { name: "CODEX_TOKEN", value: "op://Private/codex/credential", sensitive: true },
        // The same item behind two providers is one read, not two.
        { name: "OTHER_TOKEN", value: shared, sensitive: true },
      ]),
    ];

    expect(Array.from(collectProviderSecretReferences(environments))).toEqual([
      shared,
      "op://Private/codex/credential",
    ]);
  });

  it("is empty when nothing reads from a secret store", () => {
    expect(
      Array.from(
        collectProviderSecretReferences([
          decodeEnvironment([{ name: "HOME", value: "/home/u" }]),
          undefined,
        ]),
      ),
    ).toEqual([]);
  });
});
