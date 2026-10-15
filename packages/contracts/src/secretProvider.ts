import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import { TrimmedNonEmptyString, TrimmedString } from "./baseSchemas.ts";
import { OnePasswordAccount } from "./onePassword.ts";
import { SecretProviderId } from "./secretProviderId.ts";
import { secretSetting } from "./settingsSecret.ts";

const SECRET_PROVIDER_LABEL_MAX_CHARS = 64;
const SECRET_PROVIDER_REFERENCE_MAX_CHARS = 1024;
const OPENBAO_ADDRESS_MAX_CHARS = 1024;
const OPENBAO_PATH_MAX_CHARS = 253;
const OPENBAO_MOUNT_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._/-]*$/;

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

function secretProviderReferenceIssue(value: string): string | undefined {
  if (value.length === 0) return "Secret reference must not be empty.";
  if (value !== value.trim()) return "Secret reference must not start or end with whitespace.";
  if (hasControlCharacter(value)) return "Secret reference must not contain control characters.";
  // 1Password references are resolved through an `op inject` template, where
  // `{{` and `}}` delimit a reference.
  if (value.includes("{{") || value.includes("}}")) {
    return "Secret reference must not contain {{ or }}.";
  }
  return undefined;
}

export const SecretProviderLabel = TrimmedNonEmptyString.check(
  Schema.isMaxLength(SECRET_PROVIDER_LABEL_MAX_CHARS),
);
export type SecretProviderLabel = typeof SecretProviderLabel.Type;

export const OpenBaoAddress = TrimmedNonEmptyString.check(
  Schema.isMaxLength(OPENBAO_ADDRESS_MAX_CHARS),
  Schema.makeFilter((value: string) =>
    isHttpUrl(value) ? true : "OpenBao address must be an http or https URL.",
  ),
);
export type OpenBaoAddress = typeof OpenBaoAddress.Type;

/** A KV secrets engine mount. It is passed to the CLI as an argument, so it can never start with `-`. */
export const OpenBaoMount = TrimmedString.check(
  Schema.isMaxLength(OPENBAO_PATH_MAX_CHARS),
  Schema.makeFilter((value: string) =>
    OPENBAO_MOUNT_PATTERN.test(value)
      ? true
      : "OpenBao mount must start with a letter or digit and contain no spaces.",
  ),
);
export type OpenBaoMount = typeof OpenBaoMount.Type;

export const OpenBaoNamespace = TrimmedNonEmptyString.check(
  Schema.isMaxLength(OPENBAO_PATH_MAX_CHARS),
);
export type OpenBaoNamespace = typeof OpenBaoNamespace.Type;

/** Empty means the `bao` CLI uses its own login. */
export const OpenBaoToken = TrimmedString;
export type OpenBaoToken = typeof OpenBaoToken.Type;

export const OnePasswordSecretProviderConfig = Schema.Struct({
  kind: Schema.Literal("1password"),
  label: SecretProviderLabel,
  account: OnePasswordAccount,
});
export type OnePasswordSecretProviderConfig = typeof OnePasswordSecretProviderConfig.Type;

export const OpenBaoSecretProviderConfig = Schema.Struct({
  kind: Schema.Literal("openbao"),
  label: SecretProviderLabel,
  address: OpenBaoAddress,
  mount: OpenBaoMount.pipe(Schema.withDecodingDefault(Effect.succeed("secret"))),
  namespace: Schema.optionalKey(OpenBaoNamespace),
  token: Schema.optionalKey(OpenBaoToken).pipe(secretSetting),
});
export type OpenBaoSecretProviderConfig = typeof OpenBaoSecretProviderConfig.Type;

export const SecretProviderConfig = Schema.Union([
  OnePasswordSecretProviderConfig,
  OpenBaoSecretProviderConfig,
]);
export type SecretProviderConfig = typeof SecretProviderConfig.Type;

export const SecretProviderKind = Schema.Literals(["1password", "openbao"]);
export type SecretProviderKind = SecretProviderConfig["kind"];

/**
 * A secret's location inside a named provider: `op://vault/item/field` for
 * 1Password, `path/to/secret#field` for OpenBao. The format is checked when
 * the reference is resolved, against the provider it names.
 */
export const SecretProviderReference = Schema.String.check(
  Schema.isMaxLength(SECRET_PROVIDER_REFERENCE_MAX_CHARS),
  Schema.makeFilter((value: string) => secretProviderReferenceIssue(value) ?? true),
).pipe(Schema.brand("SecretProviderReference"));
export type SecretProviderReference = typeof SecretProviderReference.Type;

export const SecretProviderSecretSource = Schema.Struct({
  kind: Schema.Literal("secret-provider"),
  provider: SecretProviderId,
  reference: SecretProviderReference,
});
export type SecretProviderSecretSource = typeof SecretProviderSecretSource.Type;
