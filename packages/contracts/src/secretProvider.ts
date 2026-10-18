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
const AWS_REGION_PATTERN = /^[a-z]{2}(-[a-z]+)+-\d+$/;
const AWS_PROFILE_MAX_CHARS = 64;

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

export const OpenBaoToken = TrimmedString;
export type OpenBaoToken = typeof OpenBaoToken.Type;

/**
 * Exactly one place the server takes the token from: the provider's stored
 * token, the `BAO_TOKEN` variable of the server process, or the login the
 * `bao` CLI holds on that machine. Nothing falls back to another source.
 */
export const OpenBaoAuth = Schema.Literals(["token", "env", "cli"]);
export type OpenBaoAuth = typeof OpenBaoAuth.Type;

/** The region the provider reads in, such as `us-east-1`. Secrets Manager is regional, so one provider is one region. */
export const AwsRegion = TrimmedNonEmptyString.check(
  Schema.makeFilter((value: string) =>
    AWS_REGION_PATTERN.test(value) ? true : "AWS region must look like us-east-1.",
  ),
);
export type AwsRegion = typeof AwsRegion.Type;

/** A named profile from the server's AWS CLI configuration. It is passed to the CLI as an argument, so it can never start with `-`. */
export const AwsProfile = TrimmedNonEmptyString.check(
  Schema.isMaxLength(AWS_PROFILE_MAX_CHARS),
  Schema.makeFilter((value: string) =>
    !value.startsWith("-") && !/\s/.test(value) && !hasControlCharacter(value)
      ? true
      : "AWS profile must not start with - or contain whitespace.",
  ),
);
export type AwsProfile = typeof AwsProfile.Type;

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
  auth: OpenBaoAuth.pipe(Schema.withDecodingDefault(Effect.succeed("token" as const))),
});
export type OpenBaoSecretProviderConfig = typeof OpenBaoSecretProviderConfig.Type;

/**
 * Credentials are never stored here: the `aws` CLI on the server resolves them
 * the way it does for any other command, from the named profile when one is
 * set, otherwise from its default chain (environment, default profile, SSO,
 * instance role).
 */
export const AwsSecretsManagerSecretProviderConfig = Schema.Struct({
  kind: Schema.Literal("aws-secrets-manager"),
  label: SecretProviderLabel,
  region: AwsRegion,
  profile: Schema.optionalKey(AwsProfile),
});
export type AwsSecretsManagerSecretProviderConfig =
  typeof AwsSecretsManagerSecretProviderConfig.Type;

export const SecretProviderConfig = Schema.Union([
  OnePasswordSecretProviderConfig,
  OpenBaoSecretProviderConfig,
  AwsSecretsManagerSecretProviderConfig,
]);
export type SecretProviderConfig = typeof SecretProviderConfig.Type;

export const SecretProviderKind = Schema.Literals(["1password", "openbao", "aws-secrets-manager"]);
export type SecretProviderKind = SecretProviderConfig["kind"];

/**
 * A secret's location inside a named provider: `op://vault/item/field` for
 * 1Password, `path/to/secret#field` for OpenBao, and `name-or-arn` or
 * `name-or-arn#key` for AWS Secrets Manager. The format is checked when the
 * reference is resolved, against the provider it names.
 */
export const SecretProviderReference = Schema.String.check(
  Schema.isMaxLength(SECRET_PROVIDER_REFERENCE_MAX_CHARS),
  Schema.makeFilter((value: string) => secretProviderReferenceIssue(value) ?? true),
).pipe(Schema.brand("SecretProviderReference"));
export type SecretProviderReference = typeof SecretProviderReference.Type;

export const SecretSourceCheckResult = Schema.Union([
  Schema.TaggedStruct("Resolved", {}),
  Schema.TaggedStruct("Failed", { detail: Schema.String }),
]);
export type SecretSourceCheckResult = typeof SecretSourceCheckResult.Type;

export const SecretProviderSecretSource = Schema.Struct({
  kind: Schema.Literal("secret-provider"),
  provider: SecretProviderId,
  reference: SecretProviderReference,
});
export type SecretProviderSecretSource = typeof SecretProviderSecretSource.Type;
