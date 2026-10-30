import { SecretProviderKind, TrimmedNonEmptyString } from "@t3tools/contracts";
import { defineSecretProvider } from "@t3tools/secret-provider-core/shared/definition";
import * as Schema from "effect/Schema";

const AWS_REGION_PATTERN = /^[a-z]{2}(-[a-z]+)+-\d+$/;
const AWS_PROFILE_MAX_CHARS = 64;
const MAX_REFERENCE_LENGTH = 1024;

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

/** Secrets Manager is regional, so one provider is one region. */
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

/**
 * Credentials are never stored: the `aws` CLI on the server resolves them from the named profile
 * when one is set, otherwise from its default chain.
 */
export const AwsSecretsManagerSecretProviderConfig = Schema.Struct({
  region: AwsRegion,
  profile: Schema.optionalKey(AwsProfile),
});
export type AwsSecretsManagerSecretProviderConfig =
  typeof AwsSecretsManagerSecretProviderConfig.Type;

/** `name-or-arn` or `name-or-arn#key`; the id follows `--secret-id`, so it can never start with `-`. A secret name has no `#`, so the first one starts the key. */
export function parseAwsReference(reference: string) {
  const separator = reference.indexOf("#");
  const secretId = separator === -1 ? reference : reference.slice(0, separator);
  const key = separator === -1 ? undefined : reference.slice(separator + 1);
  if (secretId.length === 0 || secretId.startsWith("-") || key === "") return undefined;
  return { secretId, key };
}

export const definition = defineSecretProvider({
  kind: SecretProviderKind.make("aws_secrets_manager"),
  label: "AWS Secrets Manager",
  configSchema: AwsSecretsManagerSecretProviderConfig,
  isReference: (reference) =>
    reference.length <= MAX_REFERENCE_LENGTH &&
    reference === reference.trim() &&
    !hasControlCharacter(reference) &&
    parseAwsReference(reference) !== undefined,
  referencePlaceholder: "prod/openai#api_key",
  invalidReferenceMessage:
    "Use an AWS Secrets Manager reference like name-or-arn or name-or-arn#key.",
});
