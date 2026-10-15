import { describe, expect, it } from "@effect/vitest";
import { ProviderInstanceEnvironment, SecretProviderId } from "@t3tools/contracts";
import * as Schema from "effect/Schema";

import {
  collectProviderSecretReferences,
  hasProviderSecretReference,
  NamedProviderSecretReference,
  OnePasswordSecretReference,
  providerSecretReference,
} from "./ProviderSecretReference.ts";

const decodeEnvironment = Schema.decodeSync(ProviderInstanceEnvironment);

const ACCOUNT = "my.1password.com";

const onePasswordVariable = (name: string, reference: string, account = ACCOUNT) => ({
  name,
  value: { kind: "1password" as const, reference, account },
});

const namedVariable = (name: string, provider: string, reference: string) => ({
  name,
  value: { kind: "secret-provider" as const, provider, reference },
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

  it("reads a named provider source with its provider id", () => {
    const [variable] = decodeEnvironment([
      namedVariable("TOKEN", "team-vault", "agents/codex#api_key"),
    ]);
    expect(providerSecretReference(variable!.value)).toEqual(
      new NamedProviderSecretReference({
        provider: SecretProviderId.make("team-vault"),
        reference: "agents/codex#api_key",
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

describe("hasProviderSecretReference with named providers", () => {
  it("is true for a named provider source", () => {
    expect(
      hasProviderSecretReference(
        decodeEnvironment([namedVariable("TOKEN", "team-vault", "agents/codex#api_key")]),
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

    expect(references).toEqual([
      new OnePasswordSecretReference({
        reference: shared,
        account: "my.1password.com" as OnePasswordSecretReference["account"],
      }),
      new OnePasswordSecretReference({
        reference: shared,
        account: "acme.1password.com" as OnePasswordSecretReference["account"],
      }),
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

  it("keeps one reference behind different providers apart, and apart from 1Password", () => {
    const shared = "op://Private/shared/credential";
    const references = collectProviderSecretReferences([
      decodeEnvironment([
        namedVariable("HOME_TOKEN", "home", shared),
        namedVariable("WORK_TOKEN", "work", shared),
        onePasswordVariable("LEGACY_TOKEN", shared),
        namedVariable("HOME_AGAIN", "home", shared),
      ]),
    ]);

    expect(references).toEqual([
      new NamedProviderSecretReference({
        provider: SecretProviderId.make("home"),
        reference: shared,
      }),
      new NamedProviderSecretReference({
        provider: SecretProviderId.make("work"),
        reference: shared,
      }),
      secret(shared),
    ]);
  });
});
