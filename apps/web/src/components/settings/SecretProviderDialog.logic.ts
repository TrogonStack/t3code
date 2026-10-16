import {
  AwsProfile,
  AwsRegion,
  OnePasswordAccount,
  OnePasswordSecretReference,
  OpenBaoAddress,
  OpenBaoAuth,
  OpenBaoMount,
  OpenBaoNamespace,
  SecretProviderId,
  SecretProviderLabel,
  type SecretProviderConfig,
  type SecretProviderKind,
} from "@t3tools/contracts";
import * as Result from "effect/Result";
import * as Schema from "effect/Schema";

export const SECRET_PROVIDER_KIND_LABELS: Readonly<Record<SecretProviderKind, string>> = {
  "1password": "1Password",
  openbao: "OpenBao",
  "aws-secrets-manager": "AWS Secrets Manager",
};

export const SECRET_PROVIDER_REFERENCE_PLACEHOLDERS: Readonly<Record<SecretProviderKind, string>> =
  {
    "1password": "op://Private/OpenAI/credential",
    openbao: "apps/openai#api_key",
    "aws-secrets-manager": "prod/openai#api_key",
  };

export const DEFAULT_OPENBAO_MOUNT = "secret";

/** One line naming the kind and where it reads from, for rows and pickers. */
export function secretProviderSummary(config: SecretProviderConfig): string {
  const kind = SECRET_PROVIDER_KIND_LABELS[config.kind];
  switch (config.kind) {
    case "1password":
      return `${kind} · ${config.account}`;
    case "openbao":
      return `${kind} · ${config.address}`;
    case "aws-secrets-manager":
      return `${kind} · ${config.region}`;
  }
}

const decodeOnePasswordSecretReference = Schema.decodeResult(OnePasswordSecretReference);

/**
 * The format each kind resolves, checked in the client so a row explains the
 * mistake before the provider starts and silently gets no value.
 */
export function secretProviderReferenceIssue(
  kind: SecretProviderKind,
  reference: string,
): string | undefined {
  switch (kind) {
    case "1password": {
      const decoded = decodeOnePasswordSecretReference(reference);
      return Result.isFailure(decoded) ? decoded.failure.message : undefined;
    }
    case "openbao": {
      const separator = reference.indexOf("#");
      const path = separator === -1 ? "" : reference.slice(0, separator);
      const field = separator === -1 ? "" : reference.slice(separator + 1);
      if (path.length === 0 || field.length === 0) {
        return "OpenBao references look like path/to/secret#field.";
      }
      if (path.startsWith("-")) return "OpenBao secret path must not start with -.";
      return undefined;
    }
    case "aws-secrets-manager": {
      const separator = reference.indexOf("#");
      const secretId = separator === -1 ? reference : reference.slice(0, separator);
      const key = separator === -1 ? undefined : reference.slice(separator + 1);
      if (secretId.length === 0 || key === "") {
        return "AWS Secrets Manager references look like name-or-arn or name-or-arn#key.";
      }
      if (secretId.startsWith("-")) return "AWS secret id must not start with -.";
      return undefined;
    }
  }
}

/** Stable and readable in settings.json, so re-adding a provider under the same name restores its variables. */
export function secretProviderIdFromName(name: string): SecretProviderId | null {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug.length === 0 ? null : SecretProviderId.make(slug);
}

export interface SecretProviderDraft {
  readonly kind: SecretProviderKind;
  readonly name: string;
  readonly account: string;
  readonly address: string;
  readonly mount: string;
  readonly namespace: string;
  readonly token: string;
  readonly storedToken: string;
  readonly auth: OpenBaoAuth;
  readonly region: string;
  readonly profile: string;
}

export const EMPTY_SECRET_PROVIDER_DRAFT: SecretProviderDraft = {
  kind: "1password",
  name: "",
  account: "",
  address: "",
  mount: DEFAULT_OPENBAO_MOUNT,
  namespace: "",
  token: "",
  storedToken: "",
  auth: "token",
  region: "",
  profile: "",
};

export function secretProviderDraftFromConfig(config: SecretProviderConfig): SecretProviderDraft {
  const base = { ...EMPTY_SECRET_PROVIDER_DRAFT, kind: config.kind, name: config.label };
  switch (config.kind) {
    case "1password":
      return { ...base, account: config.account };
    case "openbao":
      return {
        ...base,
        address: config.address,
        mount: config.mount,
        namespace: config.namespace ?? "",
        storedToken: config.token ?? "",
        auth: config.auth,
      };
    case "aws-secrets-manager":
      return { ...base, region: config.region, profile: config.profile ?? "" };
  }
}

export type SecretProviderTarget =
  | { readonly mode: "add" }
  | { readonly mode: "edit"; readonly id: SecretProviderId };

function decodeField<S extends Schema.ConstraintDecoder<unknown>>(
  schema: S,
  value: string,
  emptyMessage: string,
): Result.Result<S["Type"], string> {
  if (value.trim().length === 0) return Result.fail(emptyMessage);
  const decoded = Schema.decodeResult(schema)(value.trim());
  return Result.isFailure(decoded)
    ? Result.fail(decoded.failure.message)
    : Result.succeed(decoded.success);
}

export type SecretProviderDraftField =
  | "name"
  | "account"
  | "address"
  | "mount"
  | "namespace"
  | "token"
  | "region"
  | "profile";

export type SecretProviderDraftIssues = Partial<Readonly<Record<SecretProviderDraftField, string>>>;

/**
 * The settings entry a draft adds or replaces, or every field that blocks it. Only the
 * fields of the draft's kind are checked, and each field reports one message.
 */
export function secretProviderFromDraft(
  draft: SecretProviderDraft,
  existingIds: ReadonlySet<string>,
  target: SecretProviderTarget,
): Result.Result<
  { readonly id: SecretProviderId; readonly config: SecretProviderConfig },
  SecretProviderDraftIssues
> {
  const issues: { -readonly [Field in SecretProviderDraftField]?: string } = {};

  const label = decodeField(SecretProviderLabel, draft.name, "Enter a name.");
  const id =
    target.mode === "edit"
      ? target.id
      : Result.isSuccess(label)
        ? secretProviderIdFromName(label.success)
        : null;
  if (Result.isFailure(label)) {
    issues.name = label.failure;
  } else if (id === null) {
    issues.name = "The name needs at least one letter or digit.";
  } else if (target.mode === "add" && existingIds.has(id)) {
    issues.name = `A secret provider named ${label.success} exists.`;
  }

  switch (draft.kind) {
    case "1password": {
      const account = decodeField(
        OnePasswordAccount,
        draft.account,
        "Enter the 1Password account.",
      );
      if (Result.isFailure(account)) issues.account = account.failure;
      if (
        issues.name !== undefined ||
        Result.isFailure(label) ||
        id === null ||
        Result.isFailure(account)
      ) {
        return Result.fail(issues);
      }
      return Result.succeed({
        id,
        config: { kind: "1password", label: label.success, account: account.success },
      });
    }
    case "openbao": {
      const address = decodeField(OpenBaoAddress, draft.address, "Enter the OpenBao address.");
      if (Result.isFailure(address)) issues.address = address.failure;
      const mount = decodeField(
        OpenBaoMount,
        draft.mount.trim() || DEFAULT_OPENBAO_MOUNT,
        "Enter the mount.",
      );
      if (Result.isFailure(mount)) issues.mount = mount.failure;
      const namespaceText = draft.namespace.trim();
      const namespace =
        namespaceText.length === 0 ? undefined : decodeField(OpenBaoNamespace, namespaceText, "");
      if (namespace !== undefined && Result.isFailure(namespace)) {
        issues.namespace = namespace.failure;
      }
      const token = draft.token.trim() || draft.storedToken;
      if (draft.auth === "token" && token.length === 0) {
        issues.token = "Enter the OpenBao token.";
      }
      if (
        issues.name !== undefined ||
        issues.token !== undefined ||
        Result.isFailure(label) ||
        id === null ||
        Result.isFailure(address) ||
        Result.isFailure(mount) ||
        (namespace !== undefined && Result.isFailure(namespace))
      ) {
        return Result.fail(issues);
      }
      return Result.succeed({
        id,
        config: {
          kind: "openbao",
          label: label.success,
          address: address.success,
          mount: mount.success,
          ...(namespace === undefined ? {} : { namespace: namespace.success }),
          auth: draft.auth,
          ...(draft.auth === "token" ? { token } : {}),
        },
      });
    }
    case "aws-secrets-manager": {
      const region = decodeField(AwsRegion, draft.region, "Enter the AWS region.");
      if (Result.isFailure(region)) issues.region = region.failure;
      const profileText = draft.profile.trim();
      const profile =
        profileText.length === 0 ? undefined : decodeField(AwsProfile, profileText, "");
      if (profile !== undefined && Result.isFailure(profile)) issues.profile = profile.failure;
      if (
        issues.name !== undefined ||
        Result.isFailure(label) ||
        id === null ||
        Result.isFailure(region) ||
        (profile !== undefined && Result.isFailure(profile))
      ) {
        return Result.fail(issues);
      }
      return Result.succeed({
        id,
        config: {
          kind: "aws-secrets-manager",
          label: label.success,
          region: region.success,
          ...(profile === undefined ? {} : { profile: profile.success }),
        },
      });
    }
  }
}
