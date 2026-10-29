import {
  OnePasswordAccount,
  SecretProviderId,
  SecretProviderKind,
  type StoredSecretProviderConfig,
} from "@t3tools/contracts";
import * as Result from "effect/Result";
import { describe, expect, it } from "vite-plus/test";

import {
  classifySecretProvider,
  EMPTY_SECRET_PROVIDER_DRAFT,
  secretProviderDisplayKind,
  secretProviderDraftFromConfig,
  secretProviderFromDraft,
  secretProviderIdFromName,
  secretProviderReferenceIssue,
  secretProviderSummary,
  type SecretProviderTarget,
} from "./SecretProviderDialog.logic";

const NO_IDS: ReadonlySet<string> = new Set();
const STORED = "stored-placeholder";
const ADD: SecretProviderTarget = { mode: "add" };

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
      ADD,
    );
    expect(result).toEqual(
      Result.succeed({
        id: "personal",
        config: { kind: "1password", label: "Personal", account: "my.1password.com" },
      }),
    );
  });

  it("rejects an empty name and a name that is already taken", () => {
    expect(
      secretProviderFromDraft({ ...EMPTY_SECRET_PROVIDER_DRAFT, name: "  " }, NO_IDS, ADD),
    ).toEqual(Result.fail({ name: "Enter a name.", account: "Enter the 1Password account." }));
    expect(
      secretProviderFromDraft(
        { ...EMPTY_SECRET_PROVIDER_DRAFT, name: "Work Vault", account: "my" },
        new Set(["work-vault"]),
        ADD,
      ),
    ).toEqual(Result.fail({ name: "A secret provider named Work Vault exists." }));
  });

  it("keys a provider by its slugged name and keeps the id when an edit renames it", () => {
    expect(
      secretProviderFromDraft(
        { ...EMPTY_SECRET_PROVIDER_DRAFT, name: "  1Password!  ", account: "my.1password.com" },
        NO_IDS,
        ADD,
      ),
    ).toEqual(
      Result.succeed({
        id: "1password",
        config: { kind: "1password", label: "1Password!", account: "my.1password.com" },
      }),
    );
    const id = SecretProviderId.make("personal");
    expect(
      secretProviderFromDraft(
        { ...EMPTY_SECRET_PROVIDER_DRAFT, name: "Personal (team)", account: "team.1password.com" },
        new Set(["personal"]),
        { mode: "edit", id },
      ),
    ).toEqual(
      Result.succeed({
        id,
        config: { kind: "1password", label: "Personal (team)", account: "team.1password.com" },
      }),
    );
  });

  it("requires a 1Password account", () => {
    expect(
      secretProviderFromDraft({ ...EMPTY_SECRET_PROVIDER_DRAFT, name: "Ops" }, NO_IDS, ADD),
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
      ADD,
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
      ADD,
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
      ADD,
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
        ADD,
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
      ADD,
    );
    expect(Result.isFailure(result) && Object.keys(result.failure)).toEqual(["address"]);
  });

  it("reports every invalid field together", () => {
    const result = secretProviderFromDraft(
      { ...EMPTY_SECRET_PROVIDER_DRAFT, kind: "openbao", name: " ", address: " " },
      NO_IDS,
      ADD,
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
      secretProviderFromDraft({ ...aws, name: "Production", region: " us-east-1 " }, NO_IDS, ADD),
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
      ADD,
    );
    expect(Result.isSuccess(result) && result.success.config).toEqual({
      kind: "aws-secrets-manager",
      label: "Prod",
      region: "eu-west-1",
      profile: "production",
    });
  });

  it("requires a region shaped like us-east-1", () => {
    expect(secretProviderFromDraft({ ...aws, name: "Prod" }, NO_IDS, ADD)).toEqual(
      Result.fail({ region: "Enter the AWS region." }),
    );
    const result = secretProviderFromDraft(
      { ...aws, name: "Prod", region: "useast1" },
      NO_IDS,
      ADD,
    );
    expect(Result.isFailure(result) && Object.keys(result.failure)).toEqual(["region"]);
  });

  it("rejects a profile that could be read as a flag or contains whitespace", () => {
    for (const profile of ["--debug", "my profile"]) {
      const result = secretProviderFromDraft(
        { ...aws, name: "Prod", region: "us-east-1", profile },
        NO_IDS,
        ADD,
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

describe("secretProviderDraftFromConfig", () => {
  it("fills a 1Password draft", () => {
    expect(
      secretProviderDraftFromConfig({
        kind: "1password",
        label: "Personal",
        account: OnePasswordAccount.make("my.1password.com"),
      }),
    ).toEqual({
      ...EMPTY_SECRET_PROVIDER_DRAFT,
      kind: "1password",
      name: "Personal",
      account: "my.1password.com",
    });
  });

  it("keeps a stored OpenBao token out of the typed field", () => {
    const draft = secretProviderDraftFromConfig({
      kind: "openbao",
      label: "Homelab",
      address: "http://127.0.0.1:8200",
      mount: "kv",
      namespace: "team-a",
      auth: "token",
      token: STORED,
    });
    expect(draft).toMatchObject({
      kind: "openbao",
      name: "Homelab",
      address: "http://127.0.0.1:8200",
      mount: "kv",
      namespace: "team-a",
      auth: "token",
      token: "",
      storedToken: STORED,
    });
  });

  it("has no stored token when the config carries none", () => {
    const draft = secretProviderDraftFromConfig({
      kind: "openbao",
      label: "Homelab",
      address: "http://127.0.0.1:8200",
      mount: "secret",
      auth: "env",
    });
    expect(draft).toMatchObject({ namespace: "", storedToken: "", token: "", auth: "env" });
  });

  it("fills an AWS draft", () => {
    expect(
      secretProviderDraftFromConfig({
        kind: "aws-secrets-manager",
        label: "Prod",
        region: "us-east-1",
        profile: "production",
      }),
    ).toMatchObject({
      kind: "aws-secrets-manager",
      name: "Prod",
      region: "us-east-1",
      profile: "production",
    });
  });
});

describe("secretProviderFromDraft in edit mode", () => {
  const id = SecretProviderId.make("homelab");
  const target: SecretProviderTarget = { mode: "edit", id };
  const bao = {
    ...EMPTY_SECRET_PROVIDER_DRAFT,
    kind: "openbao",
    name: "Homelab",
    address: "http://127.0.0.1:8200",
    storedToken: STORED,
  } as const;

  it("keeps the id and accepts its own name", () => {
    const result = secretProviderFromDraft(bao, new Set(["homelab"]), target);
    expect(Result.isSuccess(result) && result.success.id).toBe(id);
  });

  it("echoes the stored token when none is typed", () => {
    const result = secretProviderFromDraft(bao, NO_IDS, target);
    expect(Result.isSuccess(result) && result.success.config).toMatchObject({ token: STORED });
  });

  it("replaces the stored token with a typed one", () => {
    const result = secretProviderFromDraft({ ...bao, token: " s.new " }, NO_IDS, target);
    expect(Result.isSuccess(result) && result.success.config).toMatchObject({ token: "s.new" });
  });

  it("omits the token when authentication switches to env", () => {
    const result = secretProviderFromDraft({ ...bao, auth: "env" }, NO_IDS, target);
    expect(Result.isSuccess(result) && "token" in result.success.config).toBe(false);
  });

  it("still requires a token in add mode", () => {
    expect(secretProviderFromDraft({ ...bao, storedToken: "" }, NO_IDS, ADD)).toEqual(
      Result.fail({ token: "Enter the OpenBao token." }),
    );
  });
});

describe("upstream envelope entries", () => {
  const envelope = (kind: string, config?: unknown): StoredSecretProviderConfig => ({
    kind: SecretProviderKind.make(kind),
    label: "Personal",
    ...(config === undefined ? {} : { config }),
  });
  const onePassword = envelope("one_password", { account: "my.1password.com" });

  it("shows a one_password envelope as a 1Password provider with its account", () => {
    expect(classifySecretProvider(onePassword)).toEqual({
      kind: "one_password",
      account: "my.1password.com",
    });
    expect(secretProviderDisplayKind(onePassword)).toBe("1password");
    expect(secretProviderSummary(onePassword)).toBe("1Password · my.1password.com");
    expect(secretProviderDraftFromConfig(onePassword)).toEqual({
      ...EMPTY_SECRET_PROVIDER_DRAFT,
      name: "Personal",
      account: "my.1password.com",
    });
  });

  it("shows unknown kinds and undecodable configs as unsupported", () => {
    for (const entry of [
      envelope("vault_x", { a: 1 }),
      envelope("one_password", { account: "" }),
    ]) {
      expect(classifySecretProvider(entry)).toEqual({ kind: "unsupported" });
      expect(secretProviderDisplayKind(entry)).toBeUndefined();
    }
  });

  it("classifies a fork flat entry as flat", () => {
    const flat = {
      kind: "1password",
      label: "Home",
      account: OnePasswordAccount.make("my"),
    } as const;
    expect(classifySecretProvider(flat)).toEqual({ kind: "flat", config: flat });
    expect(secretProviderDisplayKind(flat)).toBe("1password");
  });
});
