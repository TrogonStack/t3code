import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Tracer from "effect/Tracer";

import { ClientTracingLive } from "../observability/clientTracer";
import { runtime } from "./runtime";

const readTracer = Effect.service(Tracer.Tracer);

describe("web runtime", () => {
  it.effect("installs the client tracer so client spans reach the trace proxy", () =>
    Effect.gen(function* () {
      const installed = yield* Effect.promise(() => runtime.runPromise(readTracer));

      expect(installed).toBe(yield* readTracer);
    }).pipe(Effect.provide(ClientTracingLive)),
  );
});
