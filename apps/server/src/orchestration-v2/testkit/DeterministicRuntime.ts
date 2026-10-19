import * as NodeOS from "node:os";

import * as HostProcess from "@t3tools/shared/HostProcess";
import * as Effect from "effect/Effect";
import * as Random from "effect/Random";
import { TestClock } from "effect/testing";

const DETERMINISTIC_HOME = `${NodeOS.tmpdir()}/t3-deterministic-home`;

export function provideDeterministicTestRuntime<A, E, R>(
  effect: Effect.Effect<A, E, R>,
  options: { readonly randomSeed?: number } = {},
) {
  return effect.pipe(
    Effect.provide(TestClock.layer()),
    Random.withSeed(options.randomSeed ?? 0x1234_5678),
    Effect.provideService(HostProcess.HomeDirectory, DETERMINISTIC_HOME),
  );
}
