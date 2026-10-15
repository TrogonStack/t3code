import {
  OnePasswordAccount,
  OnePasswordSecretReference,
  OpenBaoAddress,
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
};

export const SECRET_PROVIDER_REFERENCE_PLACEHOLDERS: Readonly<Record<SecretProviderKind, string>> =
  {
    "1password": "op://vault/item/field",
    openbao: "path/to/secret#field",
  };

export const DEFAULT_OPENBAO_MOUNT = "secret";

/** One line naming the kind and where it reads from, for rows and pickers. */
export function secretProviderSummary(config: SecretProviderConfig): string {
  const kind = SECRET_PROVIDER_KIND_LABELS[config.kind];
  return config.kind === "1password"
    ? `${kind} · ${config.account}`
    : `${kind} · ${config.address}`;
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
  if (kind === "1password") {
    const decoded = decodeOnePasswordSecretReference(reference);
    return Result.isFailure(decoded) ? decoded.failure.message : undefined;
  }
  const separator = reference.indexOf("#");
  const path = separator === -1 ? "" : reference.slice(0, separator);
  const field = separator === -1 ? "" : reference.slice(separator + 1);
  if (path.length === 0 || field.length === 0) {
    return "OpenBao references look like path/to/secret#field.";
  }
  if (path.startsWith("-")) return "OpenBao secret path must not start with -.";
  return undefined;
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
}

export const EMPTY_SECRET_PROVIDER_DRAFT: SecretProviderDraft = {
  kind: "1password",
  name: "",
  account: "",
  address: "",
  mount: DEFAULT_OPENBAO_MOUNT,
  namespace: "",
  token: "",
};

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

/** The settings entry a draft adds, or the first reason it cannot be added yet. */
export function secretProviderFromDraft(
  draft: SecretProviderDraft,
  existingIds: ReadonlySet<string>,
): Result.Result<{ readonly id: SecretProviderId; readonly config: SecretProviderConfig }, string> {
  const label = decodeField(SecretProviderLabel, draft.name, "Enter a name.");
  if (Result.isFailure(label)) return Result.fail(label.failure);
  const id = secretProviderIdFromName(label.success);
  if (id === null) return Result.fail("The name needs at least one letter or digit.");
  if (existingIds.has(id)) return Result.fail(`A secret provider named ${label.success} exists.`);

  if (draft.kind === "1password") {
    const account = decodeField(OnePasswordAccount, draft.account, "Choose a 1Password account.");
    if (Result.isFailure(account)) return Result.fail(account.failure);
    return Result.succeed({
      id,
      config: { kind: "1password", label: label.success, account: account.success },
    });
  }

  const address = decodeField(OpenBaoAddress, draft.address, "Enter the OpenBao address.");
  if (Result.isFailure(address)) return Result.fail(address.failure);
  const mount = decodeField(
    OpenBaoMount,
    draft.mount.trim() || DEFAULT_OPENBAO_MOUNT,
    "Enter the mount.",
  );
  if (Result.isFailure(mount)) return Result.fail(mount.failure);
  const namespaceText = draft.namespace.trim();
  const namespace =
    namespaceText.length === 0 ? undefined : decodeField(OpenBaoNamespace, namespaceText, "");
  if (namespace !== undefined && Result.isFailure(namespace)) return Result.fail(namespace.failure);
  const token = draft.token.trim();
  return Result.succeed({
    id,
    config: {
      kind: "openbao",
      label: label.success,
      address: address.success,
      mount: mount.success,
      ...(namespace === undefined ? {} : { namespace: namespace.success }),
      ...(token.length === 0 ? {} : { token }),
    },
  });
}
