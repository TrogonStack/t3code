import {
  SecretProviderId,
  SecretProviderKind,
  SecretProviderLabel,
  type SecretProviderConfig,
} from "@t3tools/contracts";
import {
  AwsProfile,
  AwsRegion,
  AwsSecretsManagerSecretProviderConfig,
} from "@t3tools/secret-provider-aws/shared/definition";
import {
  OnePasswordSecretProviderConfig,
  type OnePasswordAccount,
} from "@t3tools/secret-provider-onepassword/shared/definition";
import {
  OpenBaoAddress,
  OpenBaoMount,
  OpenBaoNamespace,
  OpenBaoSecretProviderConfig,
  type OpenBaoAuth,
} from "@t3tools/secret-provider-openbao/shared/definition";
import * as Option from "effect/Option";
import * as Result from "effect/Result";
import * as Schema from "effect/Schema";

const decodeSecretProviderId = Schema.decodeOption(SecretProviderId);
const decodeOnePasswordConfig = Schema.decodeUnknownOption(OnePasswordSecretProviderConfig);
const decodeOpenBaoConfig = Schema.decodeUnknownOption(OpenBaoSecretProviderConfig);
const decodeAwsConfig = Schema.decodeUnknownOption(AwsSecretsManagerSecretProviderConfig);

export const SECRET_PROVIDER_KINDS = ["one_password", "openbao", "aws_secrets_manager"] as const;
export type SupportedSecretProviderKind = (typeof SECRET_PROVIDER_KINDS)[number];

export const SECRET_PROVIDER_KIND_LABELS: Readonly<Record<SupportedSecretProviderKind, string>> = {
  one_password: "1Password",
  openbao: "OpenBao",
  aws_secrets_manager: "AWS Secrets Manager",
};

export const DEFAULT_OPENBAO_MOUNT = "secret";

export type ClassifiedSecretProvider =
  | {
      readonly kind: "one_password";
      readonly account: OnePasswordAccount;
    }
  | {
      readonly kind: "openbao";
      readonly config: OpenBaoSecretProviderConfig;
    }
  | {
      readonly kind: "aws_secrets_manager";
      readonly config: AwsSecretsManagerSecretProviderConfig;
    }
  | { readonly kind: "unsupported" };

/** Unknown kinds and configs that no longer decode are shown read-only; removal stays available. */
export function classifySecretProvider(provider: SecretProviderConfig): ClassifiedSecretProvider {
  switch (provider.kind) {
    case "one_password": {
      const decoded = decodeOnePasswordConfig(provider.config);
      return Option.isSome(decoded)
        ? { kind: "one_password", account: decoded.value.account }
        : { kind: "unsupported" };
    }
    case "openbao": {
      const decoded = decodeOpenBaoConfig(provider.config);
      return Option.isSome(decoded)
        ? { kind: "openbao", config: decoded.value }
        : { kind: "unsupported" };
    }
    case "aws_secrets_manager": {
      const decoded = decodeAwsConfig(provider.config);
      return Option.isSome(decoded)
        ? { kind: "aws_secrets_manager", config: decoded.value }
        : { kind: "unsupported" };
    }
    default:
      return { kind: "unsupported" };
  }
}

/** One line naming where a provider reads from, for rows. */
export function secretProviderDescription(classified: ClassifiedSecretProvider): string | null {
  switch (classified.kind) {
    case "one_password":
      return null;
    case "openbao":
      return [
        `OpenBao · ${classified.config.address}`,
        `mount ${classified.config.mount}`,
        ...(classified.config.namespace === undefined
          ? []
          : [`namespace ${classified.config.namespace}`]),
        ...(classified.config.auth === "env"
          ? ["BAO_TOKEN"]
          : classified.config.auth === "cli"
            ? ["bao CLI login"]
            : []),
      ].join(" · ");
    case "aws_secrets_manager":
      return [
        `AWS Secrets Manager · ${classified.config.region}`,
        ...(classified.config.profile === undefined
          ? []
          : [`profile ${classified.config.profile}`]),
      ].join(" · ");
    case "unsupported":
      return null;
  }
}

/** Stable and readable in settings.json, so re-adding a provider under the same name restores its variables. */
function secretProviderIdFromName(name: string): SecretProviderId | null {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 64)
    .replace(/-$/, "");
  return Option.getOrNull(decodeSecretProviderId(slug));
}

export interface SecretProviderDraft {
  readonly kind: SupportedSecretProviderKind;
  readonly name: string;
  readonly account: string;
  readonly address: string;
  readonly mount: string;
  readonly namespace: string;
  readonly auth: OpenBaoAuth;
  readonly token: string;
  /** The token already stored with the provider; kept when none is typed. */
  readonly storedToken: string;
  readonly region: string;
  readonly profile: string;
}

export const EMPTY_SECRET_PROVIDER_DRAFT: SecretProviderDraft = {
  kind: "one_password",
  name: "",
  account: "",
  address: "",
  mount: DEFAULT_OPENBAO_MOUNT,
  namespace: "",
  auth: "token",
  token: "",
  storedToken: "",
  region: "",
  profile: "",
};

export function draftFromSecretProvider(provider: SecretProviderConfig): SecretProviderDraft {
  const classified = classifySecretProvider(provider);
  const base = { ...EMPTY_SECRET_PROVIDER_DRAFT, name: provider.label };
  switch (classified.kind) {
    case "one_password":
      return { ...base, account: classified.account };
    case "openbao":
      return {
        ...base,
        kind: "openbao",
        address: classified.config.address,
        mount: classified.config.mount,
        namespace: classified.config.namespace ?? "",
        auth: classified.config.auth,
        storedToken: classified.config.token ?? "",
      };
    case "aws_secrets_manager":
      return {
        ...base,
        kind: "aws_secrets_manager",
        region: classified.config.region,
        profile: classified.config.profile ?? "",
      };
    case "unsupported":
      return base;
  }
}

export type SecretProviderTarget =
  | { readonly mode: "add" }
  | { readonly mode: "edit"; readonly id: SecretProviderId };

export type SecretProviderDraftIssues = {
  readonly [Field in Exclude<keyof SecretProviderDraft, "kind" | "auth" | "storedToken">]?: string;
};

function decodeField<S extends Schema.ConstraintDecoder<unknown>>(
  schema: S,
  value: string,
  emptyMessage: string,
): Result.Result<S["Type"], string> {
  const trimmed = value.trim();
  if (trimmed.length === 0) return Result.fail(emptyMessage);
  return Result.mapError(Schema.decodeResult(schema)(trimmed), (error) => error.message);
}

function optionalField<S extends Schema.ConstraintDecoder<unknown>>(
  schema: S,
  value: string,
): Result.Result<S["Type"] | undefined, string> {
  return value.trim().length === 0 ? Result.succeed(undefined) : decodeField(schema, value, "");
}

const issuesOf = (
  fields: Partial<Record<keyof SecretProviderDraftIssues, Result.Result<unknown, string>>>,
): SecretProviderDraftIssues =>
  Object.fromEntries(
    Object.entries(fields).flatMap(([field, result]) =>
      result !== undefined && Result.isFailure(result) ? [[field, result.failure]] : [],
    ),
  );

/** The settings entry a draft adds or replaces, or every field that blocks it, one message each. */
export function secretProviderFromDraft(
  draft: SecretProviderDraft,
  existingIds: ReadonlySet<string>,
  target: SecretProviderTarget,
): Result.Result<
  { readonly id: SecretProviderId; readonly config: SecretProviderConfig },
  SecretProviderDraftIssues
> {
  const label = decodeField(SecretProviderLabel, draft.name, "Enter a name.");

  let id: SecretProviderId | null = null;
  let nameIssue: string | undefined;
  if (Result.isFailure(label)) {
    nameIssue = label.failure;
  } else {
    id = target.mode === "edit" ? target.id : secretProviderIdFromName(label.success);
    if (id === null) nameIssue = "The name needs at least one letter or digit.";
    else if (target.mode === "add" && existingIds.has(id))
      nameIssue = "A secret provider with this name exists.";
  }

  const done = (config: unknown, issues: SecretProviderDraftIssues) =>
    id === null || Result.isFailure(label) || Object.keys(issues).length > 0
      ? Result.fail(issues)
      : Result.succeed({
          id,
          config: {
            kind: SecretProviderKind.make(draft.kind),
            label: label.success,
            config,
          },
        });
  const withName = (issues: SecretProviderDraftIssues): SecretProviderDraftIssues => ({
    ...(nameIssue === undefined ? {} : { name: nameIssue }),
    ...issues,
  });

  switch (draft.kind) {
    case "one_password": {
      const account = decodeField(
        OnePasswordSecretProviderConfig.fields.account,
        draft.account,
        "Enter the 1Password account.",
      );
      return done(
        { account: Result.isSuccess(account) ? account.success : undefined },
        withName(issuesOf({ account })),
      );
    }
    case "openbao": {
      const address = decodeField(OpenBaoAddress, draft.address, "Enter the OpenBao address.");
      const mount = decodeField(
        OpenBaoMount,
        draft.mount.trim() || DEFAULT_OPENBAO_MOUNT,
        "Enter the mount.",
      );
      const namespace = optionalField(OpenBaoNamespace, draft.namespace);
      const token = draft.token.trim() || draft.storedToken;
      const tokenIssue =
        draft.auth === "token" && token.length === 0 ? "Enter the OpenBao token." : undefined;
      const issues = withName({
        ...issuesOf({ address, mount, namespace }),
        ...(tokenIssue === undefined ? {} : { token: tokenIssue }),
      });
      return done(
        {
          address: Result.isSuccess(address) ? address.success : undefined,
          mount: Result.isSuccess(mount) ? mount.success : undefined,
          ...(Result.isSuccess(namespace) && namespace.success !== undefined
            ? { namespace: namespace.success }
            : {}),
          auth: draft.auth,
          ...(draft.auth === "token" ? { token } : {}),
        },
        issues,
      );
    }
    case "aws_secrets_manager": {
      const region = decodeField(AwsRegion, draft.region, "Enter the AWS region.");
      const profile = optionalField(AwsProfile, draft.profile);
      const issues = withName(issuesOf({ region, profile }));
      return done(
        {
          region: Result.isSuccess(region) ? region.success : undefined,
          ...(Result.isSuccess(profile) && profile.success !== undefined
            ? { profile: profile.success }
            : {}),
        },
        issues,
      );
    }
  }
}
