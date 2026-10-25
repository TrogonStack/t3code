import { assert, it } from "@effect/vitest";
import {
  type ProviderInstanceEnvironment,
  SecretProviderId,
  SecretProviderReference,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";

import {
  type ProviderSecretResolverShape,
  ProviderSecretResolver,
  resolveSettingsSecret,
} from "../provider/ProviderSecretResolver.ts";
import { resolveSourceControlHostSecrets } from "./sourceControlHostSecrets.ts";

const source = (reference: string) => ({
  kind: "secret-provider" as const,
  provider: SecretProviderId.make("homelab"),
  reference: SecretProviderReference.make(reference),
});

const secretsByReference = new Map([
  ["github#token", "ghp_from_provider"],
  ["bitbucket#token", "bb_from_provider"],
]);

const fakeResolver = Effect.gen(function* () {
  const fallback = yield* ProviderSecretResolver;
  const resolve: ProviderSecretResolverShape["resolve"] = (
    environment: ProviderInstanceEnvironment | undefined,
  ) =>
    Effect.sync(() => {
      const variables = [];
      const unresolved = [];
      for (const variable of environment ?? []) {
        const secret =
          typeof variable.value === "object" && variable.value.kind === "secret-provider"
            ? secretsByReference.get(variable.value.reference)
            : undefined;
        if (secret === undefined) unresolved.push(variable.name);
        else variables.push({ name: variable.name, value: secret, sensitive: true });
      }
      return { variables, unresolved };
    });
  return { ...fallback, resolve };
});

it.effect("replaces credential references with the secrets they name", () =>
  Effect.gen(function* () {
    const resolver = yield* fakeResolver;
    const hosts = {
      github: { tokens: { "github.com": source("github#token"), "ghe.acme.test": "ghp_literal" } },
      bitbucket: { email: "me@example.test", accessToken: source("bitbucket#token") },
      gitcafe: { token: source("missing#token") },
    };
    const resolved = yield* resolveSourceControlHostSecrets(hosts, (value) =>
      resolveSettingsSecret(resolver, value),
    );
    assert.deepEqual(resolved, {
      github: { tokens: { "github.com": "ghp_from_provider", "ghe.acme.test": "ghp_literal" } },
      bitbucket: { email: "me@example.test", accessToken: "bb_from_provider" },
      gitcafe: { token: "" },
    });
  }),
);

it.effect("hands back the same settings when no credential names a secret", () =>
  Effect.gen(function* () {
    const resolver = yield* fakeResolver;
    const hosts = { github: { tokens: { "github.com": "ghp_literal" } } };
    const resolved = yield* resolveSourceControlHostSecrets(hosts, (value) =>
      resolveSettingsSecret(resolver, value),
    );
    assert.strictEqual(resolved, hosts);
  }),
);
