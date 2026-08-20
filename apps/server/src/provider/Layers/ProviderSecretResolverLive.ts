/**
 * ProviderSecretResolverLive: 1Password-backed implementation of
 * `ProviderSecretResolver`.
 *
 * Resolution is `op read <reference>`, which is the same command the user
 * would run by hand and inherits their existing `op` session, so there is no
 * second place to configure credentials. Reads run one at a time: two
 * concurrent reads against a locked vault stack up two biometric prompts.
 *
 * The prompt is charged per `op` invocation rather than per secret, so reading
 * one reference at a time makes the whole fleet cost one authorization each.
 * `prime` exists for that: `op inject` substitutes any number of references in
 * a single process, so the caller that is about to build every instance pays
 * one prompt for all of them.
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
import * as Stream from "effect/Stream";
import * as NodeCrypto from "node:crypto";
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

/**
 * Read many references in one `op inject`.
 *
 * `op inject` substitutes references inside a template, so the template is the
 * references themselves joined by a separator, and the output is the secrets
 * in the same order. The separator is random per call because a secret can
 * contain anything at all, newlines included: splitting on a fixed marker
 * would let a secret that happened to contain it shift every later value.
 *
 * Returns `undefined` when the batch cannot be trusted as a whole, which
 * includes a single bad reference, since `op` resolves the template or fails
 * it. The caller treats that as "not primed" and reads one at a time, which is
 * both the per-variable failure isolation and the way the user finds out which
 * reference is the broken one.
 */
const readSecretsTogether = Effect.fn("readSecretsTogether")(function* (
  references: ReadonlyArray<string>,
) {
  const separator = `__t3-secret-${NodeCrypto.randomUUID()}__`;
  const template = references.map((reference) => `{{ ${reference} }}`).join(separator);
  const spawnCommand = yield* resolveSpawnCommand(ONE_PASSWORD_BINARY, ["inject"]);
  const result = yield* spawnAndCollect(
    ONE_PASSWORD_BINARY,
    ChildProcess.make(spawnCommand.command, spawnCommand.args, {
      shell: spawnCommand.shell,
      stdin: Stream.make(new TextEncoder().encode(template)),
    }),
  );
  if (result.code !== 0) {
    // `op` names the reference it could not resolve on stderr and never echoes
    // a secret, so this is safe to log verbatim.
    yield* Effect.logWarning("Could not batch-read provider secrets from 1Password", {
      references: references.length,
      exitCode: result.code,
      detail: result.stderr.trim(),
    });
    return undefined;
  }
  const values = result.stdout.split(separator);
  if (values.length !== references.length) {
    yield* Effect.logWarning("1Password returned an unexpected number of provider secrets", {
      expected: references.length,
      received: values.length,
    });
    return undefined;
  }
  return values.map((value) => {
    const secret = value.trim();
    return secret.length > 0 ? secret : undefined;
  });
});

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
    // The service tag declares `prime` as `Effect<void>`, so the spawner it
    // needs is captured here rather than asked of the caller, the same way
    // the cache's own lookup captures it.
    const spawnerContext = yield* Effect.context<ChildProcessSpawner.ChildProcessSpawner>();

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

    const prime: ProviderSecretResolverShape["prime"] = (references) =>
      Effect.gen(function* () {
        const wanted: Array<string> = [];
        for (const reference of new Set(references)) {
          if (!(yield* Cache.has(cache, reference))) {
            wanted.push(reference);
          }
        }
        // One reference costs one prompt whichever command reads it, so there
        // is nothing to save and `op read` gives the better error.
        if (wanted.length < 2) {
          return;
        }
        const values = yield* readSecretsTogether(wanted).pipe(
          Effect.timeoutOption(SECRET_READ_TIMEOUT),
          Effect.map(Option.getOrUndefined),
          Effect.catch((error) =>
            Effect.logWarning("Could not run 1Password to batch-read provider secrets", {
              references: wanted.length,
              detail: String(error),
            }).pipe(Effect.as(undefined)),
          ),
        );
        if (values === undefined) {
          return;
        }
        yield* Effect.forEach(
          wanted,
          (reference, index) => Cache.set(cache, reference, values[index]),
          {
            discard: true,
          },
        );
      }).pipe(Effect.provideContext(spawnerContext));

    return { resolve, prime, invalidate: Cache.invalidateAll(cache) };
  }),
);
