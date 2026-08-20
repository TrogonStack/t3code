/**
 * ProviderSecretResolverLive: 1Password-backed implementation of
 * `ProviderSecretResolver`.
 *
 * Resolution is `op read <reference>`, which is the same command the user
 * would run by hand and inherits their existing `op` session, so there is no
 * second place to configure credentials. Reads run one at a time: two
 * concurrent reads against a locked vault stack up two biometric prompts.
 *
 * Failures are cached alongside successes. If the vault is locked when the
 * first thread starts, every later thread in that session would otherwise
 * re-prompt; caching the miss keeps the failure quiet and puts recovery on
 * the refresh button, which is where the user already looks when a provider
 * shows as logged out.
 *
 * @module provider/Layers/ProviderSecretResolverLive
 */
import type {
  ProviderInstanceEnvironment,
  ProviderInstanceEnvironmentVariable,
} from "@t3tools/contracts";
import * as Cache from "effect/Cache";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import { ChildProcess, ChildProcessSpawner } from "effect/unstable/process";
import { resolveSpawnCommand } from "@t3tools/shared/shell";

import { hasProviderSecretReference, providerSecretReference } from "../ProviderSecretReference.ts";
import { spawnAndCollect } from "../providerSnapshot.ts";
import {
  ProviderSecretResolver,
  type ProviderSecretResolverShape,
} from "../Services/ProviderSecretResolver.ts";

const ONE_PASSWORD_BINARY = "op";

/**
 * Bound on a single `op read`. Long enough for a user to reach for the
 * fingerprint reader, short enough that a vault that will never answer does
 * not wedge the instance rebuild that is waiting on it.
 */
const SECRET_READ_TIMEOUT = Duration.seconds(45);

/**
 * References are small and few - one per provider credential. The cap only
 * exists so a settings file that somehow accumulates hundreds cannot pin
 * every secret it ever mentioned in memory.
 */
const SECRET_CACHE_CAPACITY = 64;

const readSecret = Effect.fn("readSecret")(function* (reference: string) {
  const spawnCommand = yield* resolveSpawnCommand(ONE_PASSWORD_BINARY, [
    "read",
    "--no-newline",
    reference,
  ]);
  const result = yield* spawnAndCollect(
    ONE_PASSWORD_BINARY,
    ChildProcess.make(spawnCommand.command, spawnCommand.args, { shell: spawnCommand.shell }),
  );
  if (result.code !== 0) {
    // `op` reports "not signed in", "item not found", and friends on stderr
    // and never echoes the secret itself, so this is safe to log verbatim.
    yield* Effect.logWarning("Could not read provider secret from 1Password", {
      reference,
      exitCode: result.code,
      detail: result.stderr.trim(),
    });
    return undefined;
  }
  const secret = result.stdout.trim();
  return secret.length > 0 ? secret : undefined;
});

export const ProviderSecretResolverLive: Layer.Layer<
  ProviderSecretResolver,
  never,
  ChildProcessSpawner.ChildProcessSpawner
> = Layer.effect(
  ProviderSecretResolver,
  Effect.gen(function* () {
    const cache = yield* Cache.make({
      capacity: SECRET_CACHE_CAPACITY,
      // No time to live: a resolved secret is held until the user asks for a
      // provider refresh. Expiring on a timer would reintroduce the surprise
      // biometric prompt mid-session that the cache exists to remove.
      lookup: (reference: string) =>
        readSecret(reference).pipe(
          Effect.timeoutOption(SECRET_READ_TIMEOUT),
          Effect.map(Option.getOrUndefined),
          Effect.catch((error) =>
            Effect.logWarning("Could not run 1Password to read a provider secret", {
              reference,
              detail: String(error),
            }).pipe(Effect.as(undefined)),
          ),
        ),
    });

    const resolve: ProviderSecretResolverShape["resolve"] = (environment) =>
      Effect.gen(function* () {
        if (!hasProviderSecretReference(environment)) {
          return { variables: environment, unresolved: [] };
        }
        const resolved: Array<ProviderInstanceEnvironmentVariable> = [];
        const unresolved: Array<string> = [];
        for (const variable of environment ?? []) {
          const reference = providerSecretReference(variable.value);
          if (reference === undefined) {
            resolved.push(variable);
            continue;
          }
          const secret = yield* Cache.get(cache, reference);
          if (secret === undefined) {
            unresolved.push(variable.name);
            continue;
          }
          resolved.push({ ...variable, value: secret });
        }
        return { variables: resolved as ProviderInstanceEnvironment, unresolved };
      });

    return { resolve, invalidate: Cache.invalidateAll(cache) };
  }),
);
