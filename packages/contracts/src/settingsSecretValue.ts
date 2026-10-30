import * as Schema from "effect/Schema";

import { TrimmedNonEmptyString, TrimmedString } from "./baseSchemas.ts";
import { SecretReference } from "./secretReference.ts";

/**
 * A credential setting: the literal, which is kept in the secret store and redacted for
 * clients, or the secret reference to read it from, which is not itself a secret and is
 * stored and shown as written.
 */
export const SettingsSecretValue = Schema.Union([TrimmedString, SecretReference]);
export type SettingsSecretValue = typeof SettingsSecretValue.Type;

export const isSecretReference = Schema.is(SecretReference);

/** The literal a credential setting holds, or `""` when it names a secret instead. */
export function settingsSecretLiteral(value: SettingsSecretValue | undefined): string {
  return typeof value === "string" ? value : "";
}

export const SecretSourceCheckResult = Schema.Union([
  Schema.TaggedStruct("Resolved", {}),
  Schema.TaggedStruct("Failed", { detail: Schema.String }),
]);
export type SecretSourceCheckResult = typeof SecretSourceCheckResult.Type;

/** A 1Password account signed in on the server, as `op account list` reports it. */
export const OnePasswordAccountSummary = Schema.Struct({
  account: TrimmedNonEmptyString,
  email: Schema.String,
});
export type OnePasswordAccountSummary = typeof OnePasswordAccountSummary.Type;
