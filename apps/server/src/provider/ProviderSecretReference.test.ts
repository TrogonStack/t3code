import { describe, expect, it } from "@effect/vitest";
import { ProviderInstanceEnvironment } from "@t3tools/contracts";
import * as Schema from "effect/Schema";

import {
  collectProviderSecretReferences,
  hasProviderSecretReference,
  OnePasswordSecretReference,
  providerSecretReference,
} from "./ProviderSecretReference.ts";

const decodeEnvironment = Schema.decodeSync(ProviderInstanceEnvironment);

const legacy = (reference: string) =>
  new OnePasswordSecretReference({ reference, account: undefined });

const onePasswordVariable = (name: string, reference: string, account: string) => ({
  name,
  value: { kind: "1password" as const, reference, account },
});

describe("providerSecretReference", () => {
  it("reads a 1Password source with its account", () => {
    const [variable] = decodeEnvironment([
      onePasswordVariable("TOKEN", "op://Private/claude-code/credential", "my.1password.com"),
    ]);
    expect(providerSecretReference(variable!.value)).toEqual(
      new OnePasswordSecretReference({
        reference: "op://Private/claude-code/credential",
        account: "my.1password.com" as OnePasswordSecretReference["account"],
      }),
    );
  });

  it("reads a legacy op:// string from the default account", () => {
    expect(providerSecretReference("op://Private/claude-code/credential")).toEqual(
      legacy("op://Private/claude-code/credential"),
    );
  });

  it("trims a reference pasted with surrounding whitespace", () => {
    expect(providerSecretReference("  op://Private/claude-code/credential\n")).toEqual(
      legacy("op://Private/claude-code/credential"),
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

describe("hasProviderSecretReference with secret sources", () => {
  it("is true for a 1Password source", () => {
    expect(
      hasProviderSecretReference(
        decodeEnvironment([
          onePasswordVariable("TOKEN", "op://Private/item/field", "my.1password.com"),
        ]),
      ),
    ).toBe(true);
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
      legacy(shared),
      legacy("op://Private/codex/credential"),
    ]);
  });

  it("keeps one reference read from different accounts apart", () => {
    const shared = "op://Private/shared/credential";
    const references = collectProviderSecretReferences([
      decodeEnvironment([
        onePasswordVariable("HOME_TOKEN", shared, "my.1password.com"),
        onePasswordVariable("WORK_TOKEN", shared, "acme.1password.com"),
        onePasswordVariable("HOME_AGAIN", shared, "my.1password.com"),
        { name: "LEGACY_TOKEN", value: shared },
      ]),
    ]);

    expect(references.map(({ reference, account }) => [reference, account])).toEqual([
      [shared, "my.1password.com"],
      [shared, "acme.1password.com"],
      [shared, undefined],
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
