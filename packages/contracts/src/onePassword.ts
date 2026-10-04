import * as Schema from "effect/Schema";

const ONE_PASSWORD_SECRET_REFERENCE_PREFIX = "op://";
const ONE_PASSWORD_SECRET_REFERENCE_MAX_CHARS = 1024;
const ONE_PASSWORD_ACCOUNT_MAX_CHARS = 253;
const ONE_PASSWORD_ACCOUNT_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

function onePasswordSecretReferenceIssue(value: string): string | undefined {
  if (value !== value.trim()) {
    return "1Password secret reference must not start or end with whitespace.";
  }
  if (hasControlCharacter(value)) {
    return "1Password secret reference must not contain control characters.";
  }
  if (!value.startsWith(ONE_PASSWORD_SECRET_REFERENCE_PREFIX)) {
    return "1Password secret reference must start with op://.";
  }
  // The resolver embeds references in an `op inject` template, where `{{` and
  // `}}` delimit a reference.
  if (value.includes("{{") || value.includes("}}")) {
    return "1Password secret reference must not contain {{ or }}.";
  }
  if (value.length === ONE_PASSWORD_SECRET_REFERENCE_PREFIX.length) {
    return "1Password secret reference must name a vault, item, and field after op://.";
  }
  return undefined;
}

/**
 * A 1Password secret reference such as `op://vault/item/field`. Only what
 * T3 Code itself depends on is checked here; `op` validates the rest and
 * reports which part it could not resolve.
 */
export const OnePasswordSecretReference = Schema.String.check(
  Schema.isMaxLength(ONE_PASSWORD_SECRET_REFERENCE_MAX_CHARS),
  Schema.makeFilter((value: string) => onePasswordSecretReferenceIssue(value) ?? true),
).pipe(Schema.brand("OnePasswordSecretReference"));
export type OnePasswordSecretReference = typeof OnePasswordSecretReference.Type;

/**
 * The 1Password account a reference is read from, in any form `op --account`
 * accepts: account shorthand, sign-in address (`my.1password.com`), account ID,
 * or user ID. It is passed to the CLI as an argument, so it can never start
 * with `-`.
 */
export const OnePasswordAccount = Schema.String.check(
  Schema.isMaxLength(ONE_PASSWORD_ACCOUNT_MAX_CHARS),
  Schema.makeFilter((value: string) =>
    ONE_PASSWORD_ACCOUNT_PATTERN.test(value)
      ? true
      : "1Password account must be a shorthand, sign-in address, or ID with no spaces.",
  ),
).pipe(Schema.brand("OnePasswordAccount"));
export type OnePasswordAccount = typeof OnePasswordAccount.Type;

export const OnePasswordSecretSource = Schema.Struct({
  kind: Schema.Literal("1password"),
  reference: OnePasswordSecretReference,
  account: OnePasswordAccount,
});
export type OnePasswordSecretSource = typeof OnePasswordSecretSource.Type;

/** A 1Password account signed in on the server, as `op account list` reports it. */
export const OnePasswordAccountSummary = Schema.Struct({
  account: OnePasswordAccount,
  email: Schema.String,
});
export type OnePasswordAccountSummary = typeof OnePasswordAccountSummary.Type;
