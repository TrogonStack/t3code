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
        auth: "env",
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
          auth: "env",
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
      auth: "token",
      token: "s.token",
    });
  });

  it("requires the token when authentication is token", () => {
    const result = secretProviderFromDraft(
      {
        ...EMPTY_SECRET_PROVIDER_DRAFT,
        kind: "openbao",
        name: "Bao",
        address: "http://127.0.0.1:8200",
        token: " ",
      },
      NO_IDS,
    );
    expect(result).toEqual(Result.fail({ token: "Enter the OpenBao token." }));
  });

  it("drops a typed token when authentication is env or cli", () => {
    for (const auth of ["env", "cli"] as const) {
      const result = secretProviderFromDraft(
        {
          ...EMPTY_SECRET_PROVIDER_DRAFT,
          kind: "openbao",
          name: "Bao",
          address: "http://127.0.0.1:8200",
          token: "s.typed",
          auth,
        },
        NO_IDS,
      );
      expect(Result.isSuccess(result) && result.success.config).toEqual({
        kind: "openbao",
        label: "Bao",
        address: "http://127.0.0.1:8200",
        mount: "secret",
        auth,
      });
    }
  });

  it("rejects an OpenBao address that is not an http URL", () => {
    const result = secretProviderFromDraft(
      {
        ...EMPTY_SECRET_PROVIDER_DRAFT,
        kind: "openbao",
        name: "Bao",
        address: "bao.local",
        token: "s.token",
      },
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
      Result.fail({
        name: "Enter a name.",
        address: "Enter the OpenBao address.",
        token: "Enter the OpenBao token.",
      }),
    );
  });
});

describe("secretProviderFromDraft for AWS Secrets Manager", () => {
  const aws = { ...EMPTY_SECRET_PROVIDER_DRAFT, kind: "aws-secrets-manager" as const };

  it("builds a provider without a profile", () => {
    expect(
      secretProviderFromDraft({ ...aws, name: "Production", region: " us-east-1 " }, NO_IDS),
    ).toEqual(
      Result.succeed({
        id: "production",
        config: { kind: "aws-secrets-manager", label: "Production", region: "us-east-1" },
      }),
    );
  });

  it("keeps a named profile when given", () => {
    const result = secretProviderFromDraft(
      { ...aws, name: "Prod", region: "eu-west-1", profile: " production " },
      NO_IDS,
    );
    expect(Result.isSuccess(result) && result.success.config).toEqual({
      kind: "aws-secrets-manager",
      label: "Prod",
      region: "eu-west-1",
      profile: "production",
    });
  });

  it("requires a region shaped like us-east-1", () => {
    expect(secretProviderFromDraft({ ...aws, name: "Prod" }, NO_IDS)).toEqual(
      Result.fail({ region: "Enter the AWS region." }),
    );
    const result = secretProviderFromDraft({ ...aws, name: "Prod", region: "useast1" }, NO_IDS);
    expect(Result.isFailure(result) && Object.keys(result.failure)).toEqual(["region"]);
  });

  it("rejects a profile that could be read as a flag or contains whitespace", () => {
    for (const profile of ["--debug", "my profile"]) {
      const result = secretProviderFromDraft(
        { ...aws, name: "Prod", region: "us-east-1", profile },
        NO_IDS,
      );
      expect(Result.isFailure(result) && Object.keys(result.failure)).toEqual(["profile"]);
    }
  });
});

describe("secretProviderReferenceIssue", () => {
  it("accepts each kind's reference format", () => {
    expect(secretProviderReferenceIssue("1password", "op://vault/item/field")).toBeUndefined();
    expect(secretProviderReferenceIssue("openbao", "apps/claude#token")).toBeUndefined();
    expect(secretProviderReferenceIssue("aws-secrets-manager", "prod/openai")).toBeUndefined();
    expect(
      secretProviderReferenceIssue("aws-secrets-manager", "prod/openai#api_key"),
    ).toBeUndefined();
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

  it("explains an AWS Secrets Manager reference in the wrong format", () => {
    const format = "AWS Secrets Manager references look like name-or-arn or name-or-arn#key.";
    expect(secretProviderReferenceIssue("aws-secrets-manager", "")).toBe(format);
    expect(secretProviderReferenceIssue("aws-secrets-manager", "#key")).toBe(format);
    expect(secretProviderReferenceIssue("aws-secrets-manager", "prod/openai#")).toBe(format);
    expect(secretProviderReferenceIssue("aws-secrets-manager", "-x#key")).toBe(
      "AWS secret id must not start with -.",
    );
  });
});
