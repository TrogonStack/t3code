import { SecretProviderId, SecretProviderKind } from "@t3tools/contracts";
import * as Result from "effect/Result";
import { describe, expect, it } from "vite-plus/test";

import {
  EMPTY_SECRET_PROVIDER_DRAFT,
  classifySecretProvider,
  draftFromSecretProvider,
  secretProviderDescription,
  secretProviderFromDraft,
} from "./SecretProviderDialog.logic";

const ADD = { mode: "add" } as const;
const NO_IDS: ReadonlySet<string> = new Set();
const OPENBAO = { ...EMPTY_SECRET_PROVIDER_DRAFT, kind: "openbao", name: "Homelab" } as const;
const AWS = { ...EMPTY_SECRET_PROVIDER_DRAFT, kind: "aws_secrets_manager", name: "Prod" } as const;

describe("secretProviderFromDraft", () => {
  it("adds a provider keyed by its name and reports every field that blocks it", () => {
    expect(
      secretProviderFromDraft(
        { ...EMPTY_SECRET_PROVIDER_DRAFT, name: "  1Password!  ", account: "my.1password.com" },
        NO_IDS,
        ADD,
      ),
    ).toEqual(
      Result.succeed({
        id: "1password",
        config: {
          kind: "one_password",
          label: "1Password!",
          config: { account: "my.1password.com" },
        },
      }),
    );

    const blocked = secretProviderFromDraft(
      { ...EMPTY_SECRET_PROVIDER_DRAFT, name: "1Password", account: "--help" },
      new Set(["1password"]),
      ADD,
    );
    expect(Result.isFailure(blocked) && Object.keys(blocked.failure).toSorted()).toEqual([
      "account",
      "name",
    ]);
  });

  it("keeps the id when an edit renames the provider", () => {
    const id = SecretProviderId.make("personal");
    const result = secretProviderFromDraft(
      {
        ...EMPTY_SECRET_PROVIDER_DRAFT,
        name: "Personal (team)",
        account: "team.1password.com",
      },
      new Set(["personal"]),
      { mode: "edit", id },
    );
    expect(Result.isSuccess(result) && result.success.id).toBe(id);
  });

  it("builds an OpenBao provider and defaults the mount", () => {
    const result = secretProviderFromDraft(
      {
        ...OPENBAO,
        address: "https://bao.example:8200",
        mount: "",
        namespace: " ops ",
        token: "s.t",
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
          config: {
            address: "https://bao.example:8200",
            mount: "secret",
            namespace: "ops",
            auth: "token",
            token: "s.t",
          },
        },
      }),
    );
  });

  it("keeps the stored OpenBao token when none is typed, and needs one only for token auth", () => {
    const base = { ...OPENBAO, address: "https://bao.example:8200" };
    const kept = secretProviderFromDraft({ ...base, storedToken: "s.old" }, NO_IDS, ADD);
    expect(Result.isSuccess(kept) && kept.success.config.config).toMatchObject({ token: "s.old" });

    const missing = secretProviderFromDraft(base, NO_IDS, ADD);
    expect(Result.isFailure(missing) && Object.keys(missing.failure)).toEqual(["token"]);

    const cli = secretProviderFromDraft({ ...base, auth: "cli" }, NO_IDS, ADD);
    expect(Result.isSuccess(cli) && cli.success.config.config).toEqual({
      address: "https://bao.example:8200",
      mount: "secret",
      auth: "cli",
    });
  });

  it("rejects an OpenBao address that is not an http URL", () => {
    const result = secretProviderFromDraft(
      { ...OPENBAO, address: "bao.example", auth: "env" },
      NO_IDS,
      ADD,
    );
    expect(Result.isFailure(result) && Object.keys(result.failure)).toEqual(["address"]);
  });

  it("builds an AWS Secrets Manager provider with an optional profile", () => {
    expect(secretProviderFromDraft({ ...AWS, region: "us-east-1" }, NO_IDS, ADD)).toEqual(
      Result.succeed({
        id: "prod",
        config: {
          kind: "aws_secrets_manager",
          label: "Prod",
          config: { region: "us-east-1" },
        },
      }),
    );
    const result = secretProviderFromDraft(
      { ...AWS, region: "nowhere", profile: "-x" },
      NO_IDS,
      ADD,
    );
    expect(Result.isFailure(result) && Object.keys(result.failure).toSorted()).toEqual([
      "profile",
      "region",
    ]);
  });
});

describe("stored providers", () => {
  const stored = (kind: string, config: unknown) => ({
    kind: SecretProviderKind.make(kind),
    label: "Stored",
    config,
  });

  it("round-trips a provider through its draft", () => {
    for (const draft of [
      { ...EMPTY_SECRET_PROVIDER_DRAFT, name: "Stored", account: "my.1password.com" },
      { ...OPENBAO, name: "Stored", address: "https://bao.example:8200", token: "s.t" },
      { ...AWS, name: "Stored", region: "eu-west-1", profile: "prod" },
    ]) {
      const built = secretProviderFromDraft(draft, NO_IDS, ADD);
      if (Result.isFailure(built)) throw new Error("draft should build");
      const reread = draftFromSecretProvider(built.success.config);
      expect(reread.kind).toBe(draft.kind);
      expect(secretProviderFromDraft(reread, NO_IDS, ADD)).toEqual(built);
    }
  });

  it("describes where each kind reads from and shows unknown kinds read-only", () => {
    const openbao = classifySecretProvider(
      stored("openbao", { address: "https://bao.example", namespace: "ops", auth: "cli" }),
    );
    expect(secretProviderDescription(openbao)).toBe(
      "OpenBao · https://bao.example · mount secret · namespace ops · bao CLI login",
    );
    const aws = classifySecretProvider(
      stored("aws_secrets_manager", { region: "us-east-1", profile: "prod" }),
    );
    expect(secretProviderDescription(aws)).toBe("AWS Secrets Manager · us-east-1 · profile prod");
    expect(classifySecretProvider(stored("vault", {})).kind).toBe("unsupported");
    expect(classifySecretProvider(stored("openbao", { address: "nope" })).kind).toBe("unsupported");
  });
});
