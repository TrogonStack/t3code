import type { SecretReference } from "@t3tools/contracts";
import * as SecretResolver from "@t3tools/secret-provider-core/server/SecretResolver";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

/** A resolver answering every read with `read`; by default no secret is readable. */
export const layerTest = (
  read: (references: ReadonlyArray<SecretReference>) => ReadonlyArray<string | undefined> = (
    references,
  ) => references.map(() => undefined),
) =>
  Layer.succeed(
    SecretResolver.SecretResolver,
    SecretResolver.SecretResolver.of({
      read: (references) => Effect.succeed(read(references)),
      invalidate: () => Effect.void,
      label: (provider) => Effect.succeed(provider),
    }),
  );
