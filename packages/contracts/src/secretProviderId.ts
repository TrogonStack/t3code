import * as Schema from "effect/Schema";
import { TrimmedNonEmptyString } from "./baseSchemas.ts";

// Final once shipped: older builds drop entries whose key fails here, and erase them on their next save.
// Unlike provider slugs it may start with a digit, so a provider named "1Password" fits.
const SecretProviderSlug = TrimmedNonEmptyString.check(
  Schema.isMaxLength(64),
  Schema.isPattern(/^[a-zA-Z0-9][a-zA-Z0-9_-]*$/),
);

/**
 * Key of one `settings.secretProviders` entry. Lives in its own module so
 * both the settings and the provider instance contracts can import it without
 * importing each other.
 */
export const SecretProviderId = SecretProviderSlug.pipe(Schema.brand("SecretProviderId"));
export type SecretProviderId = typeof SecretProviderId.Type;

export const SecretProviderKind = SecretProviderSlug.pipe(Schema.brand("SecretProviderKind"));
export type SecretProviderKind = typeof SecretProviderKind.Type;
