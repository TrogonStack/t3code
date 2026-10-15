import * as Schema from "effect/Schema";

/**
 * Key of one `settings.secretProviders` entry. Lives in its own module so
 * both the settings and the provider instance contracts can import it without
 * importing each other.
 */
export const SecretProviderId = Schema.String.pipe(Schema.brand("SecretProviderId"));
export type SecretProviderId = typeof SecretProviderId.Type;
