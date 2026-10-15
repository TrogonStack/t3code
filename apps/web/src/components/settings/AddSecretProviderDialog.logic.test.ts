import * as Result from "effect/Result";
import { describe, expect, it } from "vite-plus/test";

import {
  EMPTY_SECRET_PROVIDER_DRAFT,
  secretProviderFromDraft,
  secretProviderIdFromName,
  secretProviderReferenceIssue,
} from "./AddSecretProviderDialog.logic";

const NO_IDS: ReadonlySet<string> = new Set();

describe("secretProviderIdFromName", () => {
  it("slugs the name so the id stays readable in settings.json", () => {
    expect(secretProviderIdFromName("  Work 1Password!  ")).toBe("work-1password");
  });

  it("has no id for a name without letters or digits", () => {
    expect(secretProviderIdFromName(" -- ")).toBeNull();
  });
});

describe("secretProviderFromDraft", () => {
  it("builds a 1Password provider", () => {
    const result = secretProviderFromDraft(
      { ...EMPTY_SECRET_PROVIDER_DRAFT, name: "Personal", account: "my.1password.com" },
      NO_IDS,
    );
    expect(result).toEqual(
      Result.succeed({
        id: "personal",
        config: { kind: "1password", label: "Personal", account: "my.1password.com" },
      }),
    );
  });

  it("rejects an empty name and a name that is already taken", () => {
    expect(secretProviderFromDraft({ ...EMPTY_SECRET_PROVIDER_DRAFT, name: "  " }, NO_IDS)).toEqual(
      Result.fail({ name: "Enter a name.", account: "Enter the 1Password account." }),
    );
    expect(
      secretProviderFromDraft(
        { ...EMPTY_SECRET_PROVIDER_DRAFT, name: "Work Vault", account: "my" },
        new Set(["work-vault"]),
      ),
    ).toEqual(Result.fail({ name: "A secret provider named Work Vault exists." }));
  });

  it("requires a 1Password account", () => {
    expect(
      secretProviderFromDraft({ ...EMPTY_SECRET_PROVIDER_DRAFT, name: "Ops" }, NO_IDS),
    ).toEqual(Result.fail({ account: "Enter the 1Password account." }));
  });

  it("builds an OpenBao provider, defaulting the mount and omitting blank optional fields", () => {
    const result = secretProviderFromDraft(
      {
        ...EMPTY_SECRET_PROVIDER_DRAFT,
        kind: "openbao",
        name: "Homelab",
        address: " https://bao.example:8200 ",
        mount: " ",
        namespace: " ",
        token: " ",
      },
      NO_IDS,
    );
    expect(result).toEqual(
      Result.succeed({
        id: "homelab",
        config: {
          kind: "openbao",
          label: "Homelab",
          address: "https://bao.example:8200",
          mount: "secret",
          access: "api",
        },
      }),
    );
  });

  it("keeps an OpenBao namespace and token when given", () => {
    const result = secretProviderFromDraft(
      {
        ...EMPTY_SECRET_PROVIDER_DRAFT,
        kind: "openbao",
        name: "Team",
        address: "http://127.0.0.1:8200",
        mount: "kv",
        namespace: "team-a",
        token: "s.token",
      },
      NO_IDS,
    );
    expect(Result.isSuccess(result) && result.success.config).toEqual({
      kind: "openbao",
      label: "Team",
      address: "http://127.0.0.1:8200",
      mount: "kv",
      namespace: "team-a",
      token: "s.token",
      access: "api",
    });
  });

  it("rejects an OpenBao address that is not an http URL", () => {
    const result = secretProviderFromDraft(
      { ...EMPTY_SECRET_PROVIDER_DRAFT, kind: "openbao", name: "Bao", address: "bao.local" },
      NO_IDS,
    );
    expect(Result.isFailure(result) && Object.keys(result.failure)).toEqual(["address"]);
  });

  it("reports every invalid field together", () => {
    const result = secretProviderFromDraft(
      { ...EMPTY_SECRET_PROVIDER_DRAFT, kind: "openbao", name: " ", address: " " },
      NO_IDS,
    );
    expect(result).toEqual(
      Result.fail({ name: "Enter a name.", address: "Enter the OpenBao address." }),
    );
  });
});

describe("secretProviderReferenceIssue", () => {
  it("accepts each kind's reference format", () => {
    expect(secretProviderReferenceIssue("1password", "op://vault/item/field")).toBeUndefined();
    expect(secretProviderReferenceIssue("openbao", "apps/claude#token")).toBeUndefined();
  });

  it("explains a reference in the wrong format for its provider", () => {
    expect(secretProviderReferenceIssue("1password", "vault/item/field")).toBeDefined();
    expect(secretProviderReferenceIssue("openbao", "apps/claude")).toBe(
      "OpenBao references look like path/to/secret#field.",
    );
    expect(secretProviderReferenceIssue("openbao", "-x#token")).toBe(
      "OpenBao secret path must not start with -.",
    );
  });
});
