import { describe, expect, it } from "@effect/vitest";
import * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";
import * as EffectAcpErrors from "effect-acp/errors";

import {
  applyGrokAcpModelSelection,
  buildGrokAcpSpawnInput,
  grokAuthFailureFromAcpCause,
  grokAuthFromAcpAuthenticate,
  resolveGrokAcpBaseModelId,
} from "./GrokAcpSupport.ts";

describe("resolveGrokAcpBaseModelId", () => {
  it("normalizes empty and custom Grok model ids", () => {
    expect(resolveGrokAcpBaseModelId(undefined)).toBe("grok-build");
    expect(resolveGrokAcpBaseModelId("   ")).toBe("grok-build");
    expect(resolveGrokAcpBaseModelId("  grok-test-custom-model  ")).toBe("grok-test-custom-model");
  });
});

describe("buildGrokAcpSpawnInput", () => {
  it("passes the T3 Code referrer through Grok OAuth env", () => {
    const spawn = buildGrokAcpSpawnInput({ binaryPath: "/usr/local/bin/grok" }, "/tmp/project", {
      XAI_API_KEY: "secret",
      GROK_OAUTH2_REFERRER: "other-client",
    });

    expect(spawn).toEqual({
      command: "/usr/local/bin/grok",
      args: ["agent", "stdio"],
      cwd: "/tmp/project",
      env: {
        XAI_API_KEY: "secret",
        GROK_OAUTH2_REFERRER: "t3code",
      },
    });
  });
});

describe("grokAuthFromAcpAuthenticate", () => {
  it("reports the account email Grok returns from authenticate", () => {
    expect(
      grokAuthFromAcpAuthenticate({
        _meta: { email: " grok-user@example.com ", auth_mode: "Oidc", team_id: "team-1" },
      }),
    ).toEqual({ status: "authenticated", email: "grok-user@example.com" });
  });

  it("labels API key credentials when authenticate reports no account", () => {
    expect(grokAuthFromAcpAuthenticate({}, { XAI_API_KEY: "secret" })).toEqual({
      status: "authenticated",
      type: "API key",
    });
  });

  it("treats a bare authenticate success as authenticated without identity", () => {
    expect(grokAuthFromAcpAuthenticate({ _meta: { email: "   " } }, {})).toEqual({
      status: "authenticated",
    });
    expect(grokAuthFromAcpAuthenticate({ _meta: null }, {})).toEqual({ status: "authenticated" });
  });
});

describe("grokAuthFailureFromAcpCause", () => {
  it("maps an ACP auth-required failure to an unauthenticated snapshot", () => {
    const failure = grokAuthFailureFromAcpCause(
      Cause.fail(EffectAcpErrors.AcpRequestError.authRequired()),
    );
    expect(failure?.auth).toEqual({ status: "unauthenticated" });
    expect(failure?.message).toContain("not authenticated");
  });

  it("leaves unrelated ACP failures to the generic startup message", () => {
    expect(
      grokAuthFailureFromAcpCause(
        Cause.fail(EffectAcpErrors.AcpRequestError.invalidParams("session id not known")),
      ),
    ).toBeUndefined();
    expect(
      grokAuthFailureFromAcpCause(
        Cause.fail(new EffectAcpErrors.AcpSpawnError({ command: "grok", cause: "boom" })),
      ),
    ).toBeUndefined();
  });
});

describe("applyGrokAcpModelSelection", () => {
  const makeRecordingRuntime = (failure?: EffectAcpErrors.AcpError) => {
    const modelCalls: Array<string> = [];
    const runtime = {
      setSessionModel: (modelId: string) =>
        Effect.gen(function* () {
          modelCalls.push(modelId);
          if (failure) return yield* failure;
          return {};
        }),
    };
    return { runtime, modelCalls };
  };

  it.effect("calls session/set_model when the requested model differs from current", () =>
    Effect.gen(function* () {
      const { runtime, modelCalls } = makeRecordingRuntime();
      const result = yield* applyGrokAcpModelSelection({
        runtime,
        currentModelId: "grok-build",
        requestedModelId: "grok-mock-alt",
        mapError: (cause) => cause.message,
      });
      expect(modelCalls).toEqual(["grok-mock-alt"]);
      expect(result).toBe("grok-mock-alt");
    }),
  );

  it.effect("skips set_model when requested matches current", () =>
    Effect.gen(function* () {
      const { runtime, modelCalls } = makeRecordingRuntime();
      const result = yield* applyGrokAcpModelSelection({
        runtime,
        currentModelId: "grok-build",
        requestedModelId: "grok-build",
        mapError: (cause) => cause.message,
      });
      expect(modelCalls).toEqual([]);
      expect(result).toBe("grok-build");
    }),
  );

  it.effect("skips set_model when no model is requested", () =>
    Effect.gen(function* () {
      const { runtime, modelCalls } = makeRecordingRuntime();
      const result = yield* applyGrokAcpModelSelection({
        runtime,
        currentModelId: "grok-build",
        requestedModelId: undefined,
        mapError: (cause) => cause.message,
      });
      expect(modelCalls).toEqual([]);
      expect(result).toBe("grok-build");
    }),
  );

  it.effect("propagates session/set_model failures via mapError", () =>
    Effect.gen(function* () {
      const failure = EffectAcpErrors.AcpRequestError.invalidParams("session id not known");
      const { runtime } = makeRecordingRuntime(failure);
      const error = yield* Effect.flip(
        applyGrokAcpModelSelection({
          runtime,
          currentModelId: "grok-build",
          requestedModelId: "grok-mock-alt",
          mapError: (cause) => cause.message,
        }),
      );
      expect(error).toBe(failure.message);
    }),
  );
});
