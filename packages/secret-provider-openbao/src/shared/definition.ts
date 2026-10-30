import { SecretProviderKind, TrimmedNonEmptyString, TrimmedString } from "@t3tools/contracts";
import { defineSecretProvider } from "@t3tools/secret-provider-core/shared/definition";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

const ADDRESS_MAX_CHARS = 1024;
const PATH_MAX_CHARS = 253;
const MOUNT_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._/-]*$/;
const MAX_REFERENCE_LENGTH = 1024;

function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export const OpenBaoAddress = TrimmedNonEmptyString.check(
  Schema.isMaxLength(ADDRESS_MAX_CHARS),
  Schema.makeFilter((value: string) =>
    isHttpUrl(value) ? true : "OpenBao address must be an http or https URL.",
  ),
);
export type OpenBaoAddress = typeof OpenBaoAddress.Type;

/** A KV secrets engine mount. It is passed to the CLI as an argument, so it can never start with `-`. */
export const OpenBaoMount = TrimmedString.check(
  Schema.isMaxLength(PATH_MAX_CHARS),
  Schema.makeFilter((value: string) =>
    MOUNT_PATTERN.test(value)
      ? true
      : "OpenBao mount must start with a letter or digit and contain no spaces.",
  ),
);
export type OpenBaoMount = typeof OpenBaoMount.Type;

export const OpenBaoNamespace = TrimmedNonEmptyString.check(Schema.isMaxLength(PATH_MAX_CHARS));
export type OpenBaoNamespace = typeof OpenBaoNamespace.Type;

/**
 * Exactly one place the server takes the token from: the provider's stored token, the
 * `BAO_TOKEN` variable of the server process, or the login the `bao` CLI holds on that machine.
 */
export const OpenBaoAuth = Schema.Literals(["token", "env", "cli"]);
export type OpenBaoAuth = typeof OpenBaoAuth.Type;

export const OpenBaoSecretProviderConfig = Schema.Struct({
  address: OpenBaoAddress,
  mount: OpenBaoMount.pipe(Schema.withDecodingDefault(Effect.succeed("secret"))),
  namespace: Schema.optionalKey(OpenBaoNamespace),
  token: Schema.optionalKey(TrimmedString),
  auth: OpenBaoAuth.pipe(Schema.withDecodingDefault(Effect.succeed("token" as const))),
});
export type OpenBaoSecretProviderConfig = typeof OpenBaoSecretProviderConfig.Type;

/** `path/to/secret#field`; the path is a positional argument, so it can never start with `-`. */
export function parseOpenBaoReference(reference: string) {
  const separator = reference.indexOf("#");
  if (separator === -1) return undefined;
  const path = reference.slice(0, separator);
  const field = reference.slice(separator + 1);
  if (path.length === 0 || field.length === 0 || path.startsWith("-")) return undefined;
  return { path, field };
}

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

export const definition = defineSecretProvider({
  kind: SecretProviderKind.make("openbao"),
  label: "OpenBao",
  configSchema: OpenBaoSecretProviderConfig,
  isReference: (reference) =>
    reference.length <= MAX_REFERENCE_LENGTH &&
    reference === reference.trim() &&
    !hasControlCharacter(reference) &&
    parseOpenBaoReference(reference) !== undefined,
  referencePlaceholder: "path/to/secret#field",
  invalidReferenceMessage: "Use an OpenBao reference like path/to/secret#field.",
});
