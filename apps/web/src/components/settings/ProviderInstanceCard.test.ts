import * as Result from "effect/Result";
import * as Schema from "effect/Schema";
import { describe, expect, it } from "vite-plus/test";
import {
  SecretProviderConfig,
  SecretProviderId,
  SecretProviderReference,
  type OnePasswordAccount,
  type OnePasswordSecretReference,
  type ServerProviderModel,
} from "@t3tools/contracts";

import {
  deriveProviderModelsForDisplay,
  nextProviderEnvironmentWithFieldValue,
  secretSourceFromDraft,
  splitDedicatedProviderEnvironment,
} from "./ProviderInstanceCard";

describe("deriveProviderModelsForDisplay", () => {
  it("uses current config custom models instead of stale live custom rows", () => {
    const liveModels: ReadonlyArray<ServerProviderModel> = [
      {
        slug: "server-model",
        name: "Server Model",
        isCustom: false,
        capabilities: null,
      },
      {
        slug: "removed-custom",
        name: "Removed Custom",
        isCustom: true,
        capabilities: null,
      },
      {
        slug: "kept-custom",
        name: "Kept Custom",
        isCustom: true,
        capabilities: null,
      },
    ];

    expect(
      deriveProviderModelsForDisplay({
        liveModels,
        customModels: [{ slug: "kept-custom", name: "kept-custom", capabilities: null }],
      }).map((model) => model.slug),
    ).toEqual(["server-model", "kept-custom"]);
  });
});

describe("provider environment helpers", () => {
  const cursorApiKeyField = {
    name: "CURSOR_API_KEY",
    label: "Cursor API key",
    sensitive: true,
  };

  it("writes dedicated provider secrets as sensitive environment variables", () => {
    expect(
      nextProviderEnvironmentWithFieldValue(
        [{ name: "EXTRA_FLAG", value: "1", sensitive: false }],
        cursorApiKeyField,
        "  cursor-key  ",
      ),
    ).toEqual([
      { name: "EXTRA_FLAG", value: "1", sensitive: false },
      { name: "CURSOR_API_KEY", value: "cursor-key", sensitive: true },
    ]);
  });

  it("replaces redacted provider secrets without preserving redaction markers", () => {
    expect(
      nextProviderEnvironmentWithFieldValue(
        [
          {
            name: "CURSOR_API_KEY",
            value: "",
            sensitive: true,
            valueRedacted: true,
          },
        ],
        cursorApiKeyField,
        "new-key",
      ),
    ).toEqual([{ name: "CURSOR_API_KEY", value: "new-key", sensitive: true }]);
  });

  it("applies the secure field default when replacing an existing non-sensitive value", () => {
    expect(
      nextProviderEnvironmentWithFieldValue(
        [{ name: "OPENAI_API_KEY", value: "old-key", sensitive: false }],
        {
          name: "OPENAI_API_KEY",
          label: "OpenAI API key",
        },
        "new-key",
      ),
    ).toEqual([{ name: "OPENAI_API_KEY", value: "new-key", sensitive: true }]);
  });

  it("keeps a dedicated variable read from 1Password in the generic editor", () => {
    const source = {
      kind: "1password" as const,
      reference: "op://Private/cursor/credential" as OnePasswordSecretReference,
      account: "my" as OnePasswordAccount,
    };
    const environment = [
      { name: "CURSOR_API_KEY", value: source, sensitive: false },
      { name: "EXTRA_FLAG", value: "1", sensitive: false },
    ];

    expect(splitDedicatedProviderEnvironment(environment, new Set(["CURSOR_API_KEY"]))).toEqual({
      dedicated: [],
      generic: environment,
    });
  });

  it("keeps a dedicated variable read from a named secret provider in its field", () => {
    const source = {
      kind: "secret-provider" as const,
      provider: SecretProviderId.make("homelab"),
      reference: SecretProviderReference.make("apps/t3#api_key"),
    };
    const dedicated = { name: "CURSOR_API_KEY", value: source, sensitive: false };
    const generic = { name: "EXTRA_FLAG", value: source, sensitive: false };

    expect(
      splitDedicatedProviderEnvironment([dedicated, generic], new Set(["CURSOR_API_KEY"])),
    ).toEqual({ dedicated: [dedicated], generic: [generic] });
  });

  it("keeps a plain op:// value in the generic editor so its account can be picked", () => {
    const legacy = {
      name: "CURSOR_API_KEY",
      value: "op://Private/cursor/credential",
      sensitive: false,
    };
    const literal = { name: "OTHER_KEY", value: "cursor-key", sensitive: true };

    expect(
      splitDedicatedProviderEnvironment(
        [legacy, literal],
        new Set(["CURSOR_API_KEY", "OTHER_KEY"]),
      ),
    ).toEqual({ dedicated: [literal], generic: [legacy] });
  });
});

describe("secretSourceFromDraft", () => {
  const decodeConfig = Schema.decodeUnknownSync(SecretProviderConfig);
  const secretProviders = {
    [SecretProviderId.make("homelab")]: decodeConfig({
      kind: "openbao",
      label: "Homelab",
      address: "https://bao.example.test",
    }),
    [SecretProviderId.make("work")]: decodeConfig({
      kind: "1password",
      label: "Work",
      account: "work.1password.com",
    }),
  };
  const row = (
    fields: Partial<Parameters<typeof secretSourceFromDraft>[0]>,
  ): Parameters<typeof secretSourceFromDraft>[0] => ({
    source: "secret-provider",
    provider: "",
    reference: "",
    account: "",
    ...fields,
  });

  it("reads a named provider row as a secret-provider source", () => {
    expect(
      secretSourceFromDraft(
        row({ provider: "homelab", reference: "apps/t3#api_key" }),
        secretProviders,
      ),
    ).toEqual(
      Result.succeed({
        kind: "secret-provider",
        provider: "homelab",
        reference: "apps/t3#api_key",
      }),
    );
  });

  it("asks for a provider before saving", () => {
    expect(secretSourceFromDraft(row({ reference: "apps/t3#api_key" }), secretProviders)).toEqual(
      Result.fail("Choose a secret provider."),
    );
  });

  it("checks the reference against the provider's kind", () => {
    expect(
      Result.isFailure(
        secretSourceFromDraft(row({ provider: "homelab", reference: "apps/t3" }), secretProviders),
      ),
    ).toBe(true);
    expect(
      Result.isFailure(
        secretSourceFromDraft(row({ provider: "work", reference: "apps/t3#key" }), secretProviders),
      ),
    ).toBe(true);
    expect(
      Result.isSuccess(
        secretSourceFromDraft(
          row({ provider: "work", reference: "op://vault/item/field" }),
          secretProviders,
        ),
      ),
    ).toBe(true);
  });

  it("keeps a row whose provider was removed so it resolves once re-added", () => {
    expect(
      secretSourceFromDraft(row({ provider: "gone", reference: "anything" }), secretProviders),
    ).toEqual(Result.succeed({ kind: "secret-provider", provider: "gone", reference: "anything" }));
  });

  it("still converts legacy 1Password rows with their own account", () => {
    expect(
      secretSourceFromDraft(
        row({
          source: "1password",
          reference: "op://vault/item/field",
          account: "my.1password.com",
        }),
        {},
      ),
    ).toEqual(
      Result.succeed({
        kind: "1password",
        reference: "op://vault/item/field",
        account: "my.1password.com",
      }),
    );
  });
});
