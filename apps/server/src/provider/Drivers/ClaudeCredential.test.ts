import { assert, describe, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import {
  HttpClient,
  HttpClientError,
  HttpClientRequest,
  HttpClientResponse,
} from "effect/unstable/http";

import { claudeOAuthTokenFromEnvironment, verifyClaudeOAuthToken } from "./ClaudeCredential.ts";

function httpClientLayer(respond: (request: HttpClientRequest.HttpClientRequest) => Response) {
  return Layer.succeed(
    HttpClient.HttpClient,
    HttpClient.make((request) =>
      Effect.succeed(HttpClientResponse.fromWeb(request, respond(request))),
    ),
  );
}

const transportFailureLayer = Layer.succeed(
  HttpClient.HttpClient,
  HttpClient.make((request) =>
    Effect.fail(
      new HttpClientError.HttpClientError({
        reason: new HttpClientError.TransportError({ request, cause: new Error("offline") }),
      }),
    ),
  ),
);

describe("claudeOAuthTokenFromEnvironment", () => {
  it("returns the configured token", () => {
    assert.strictEqual(
      claudeOAuthTokenFromEnvironment({ CLAUDE_CODE_OAUTH_TOKEN: "sk-ant-oat01-test" }),
      "sk-ant-oat01-test",
    );
  });

  it("treats blank and missing values alike", () => {
    assert.strictEqual(claudeOAuthTokenFromEnvironment({}), undefined);
    assert.strictEqual(
      claudeOAuthTokenFromEnvironment({ CLAUDE_CODE_OAUTH_TOKEN: "   " }),
      undefined,
    );
  });

  it("trims surrounding whitespace so a copy-pasted token still matches", () => {
    assert.strictEqual(
      claudeOAuthTokenFromEnvironment({ CLAUDE_CODE_OAUTH_TOKEN: " sk-ant-oat01-test\n" }),
      "sk-ant-oat01-test",
    );
  });
});

describe("verifyClaudeOAuthToken", () => {
  it.effect("reports a live token and sends bearer auth", () =>
    Effect.gen(function* () {
      let seen: HttpClientRequest.HttpClientRequest | undefined;
      const verdict = yield* verifyClaudeOAuthToken("sk-ant-oat01-test").pipe(
        Effect.provide(
          httpClientLayer((request) => {
            seen = request;
            return Response.json({ data: [] });
          }),
        ),
      );
      assert.strictEqual(verdict, "live");
      assert.strictEqual(seen?.headers["authorization"], "Bearer sk-ant-oat01-test");
      assert.strictEqual(seen?.headers["anthropic-version"], "2023-06-01");
    }),
  );

  it.effect("never asks Anthropic about an unresolved secret reference", () =>
    Effect.gen(function* () {
      // Nothing resolved this into a credential, so a 401 would be Anthropic
      // answering a question about a string that was never a token.
      let requests = 0;
      const verdict = yield* verifyClaudeOAuthToken("op://Vault/item/credential").pipe(
        Effect.provide(
          httpClientLayer(() => {
            requests += 1;
            return new Response("no", { status: 401 });
          }),
        ),
      );
      assert.strictEqual(verdict, "unknown");
      assert.strictEqual(requests, 0);
    }),
  );

  it.effect("never asks Anthropic about a value that is not a credential", () =>
    Effect.gen(function* () {
      let requests = 0;
      const verdict = yield* verifyClaudeOAuthToken("${MY_TOKEN}").pipe(
        Effect.provide(
          httpClientLayer(() => {
            requests += 1;
            return new Response("no", { status: 401 });
          }),
        ),
      );
      assert.strictEqual(verdict, "unknown");
      assert.strictEqual(requests, 0);
    }),
  );

  it.effect("reports a rejected token on 401", () =>
    Effect.gen(function* () {
      const verdict = yield* verifyClaudeOAuthToken("sk-ant-oat01-stale").pipe(
        Effect.provide(httpClientLayer(() => new Response("unauthorized", { status: 401 }))),
      );
      assert.strictEqual(verdict, "rejected");
    }),
  );

  it.effect("stays unknown when the account merely lacks scope", () =>
    Effect.gen(function* () {
      const verdict = yield* verifyClaudeOAuthToken("sk-ant-oat01-scoped-out").pipe(
        Effect.provide(httpClientLayer(() => new Response("forbidden", { status: 403 }))),
      );
      assert.strictEqual(verdict, "unknown");
    }),
  );

  it.effect("stays unknown when Anthropic is down", () =>
    Effect.gen(function* () {
      const verdict = yield* verifyClaudeOAuthToken("sk-ant-oat01-fine").pipe(
        Effect.provide(httpClientLayer(() => new Response("boom", { status: 503 }))),
      );
      assert.strictEqual(verdict, "unknown");
    }),
  );

  it.effect("stays unknown when the request never leaves the machine", () =>
    Effect.gen(function* () {
      const verdict = yield* verifyClaudeOAuthToken("sk-ant-oat01-fine").pipe(
        Effect.provide(transportFailureLayer),
      );
      assert.strictEqual(verdict, "unknown");
    }),
  );
});
