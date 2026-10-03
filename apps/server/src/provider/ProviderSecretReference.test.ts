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

const ACCOUNT = "my.1password.com";

const onePasswordVariable = (name: string, reference: string, account = ACCOUNT) => ({
  name,
  value: { kind: "1password" as const, reference, account },
});

const secret = (reference: string) =>
  new OnePasswordSecretReference({
    reference,
    account: ACCOUNT as OnePasswordSecretReference["account"],
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

  it("treats every string as a literal, including one that looks like a reference", () => {
    expect(providerSecretReference("sk-live-token")).toBeUndefined();
    expect(providerSecretReference("/home/u/.claude/work")).toBeUndefined();
    expect(providerSecretReference("op://Private/claude-code/credential")).toBeUndefined();
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
          onePasswordVariable("CLAUDE_CODE_OAUTH_TOKEN", "op://Private/claude-code/credential"),
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
        onePasswordVariable("CLAUDE_CODE_OAUTH_TOKEN", shared),
        { name: "HOME", value: "/home/u" },
      ]),
      undefined,
      decodeEnvironment([
        onePasswordVariable("CODEX_TOKEN", "op://Private/codex/credential"),
        // The same item behind two providers is one read, not two.
        onePasswordVariable("OTHER_TOKEN", shared),
      ]),
    ];

    expect(Array.from(collectProviderSecretReferences(environments))).toEqual([
      secret(shared),
      secret("op://Private/codex/credential"),
    ]);
  });

  it("keeps one reference read from different accounts apart", () => {
    const shared = "op://Private/shared/credential";
    const references = collectProviderSecretReferences([
      decodeEnvironment([
        onePasswordVariable("HOME_TOKEN", shared, "my.1password.com"),
        onePasswordVariable("WORK_TOKEN", shared, "acme.1password.com"),
        onePasswordVariable("HOME_AGAIN", shared, "my.1password.com"),
      ]),
    ]);

    expect(references.map(({ reference, account }) => [reference, account])).toEqual([
      [shared, "my.1password.com"],
      [shared, "acme.1password.com"],
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
