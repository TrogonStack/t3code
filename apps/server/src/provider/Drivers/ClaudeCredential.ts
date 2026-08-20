/**
 * ClaudeCredential: liveness check for a configured Claude OAuth token.
 *
 * The capability probe reports which credential the CLI *found*, never whether
 * Anthropic still honours it. An expired or revoked setup token still reads as
 * `tokenSource: "CLAUDE_CODE_OAUTH_TOKEN"`, so Settings would keep claiming the
 * provider is authenticated right up until a turn fails. This module closes
 * that gap with one cheap authenticated GET.
 *
 * `GET /v1/models` is used because it is the only first-party endpoint that
 * accepts the token, costs no message quota, and answers the only question
 * worth asking: does Anthropic still accept this credential.
 *
 * @module provider/Drivers/ClaudeCredential
 */
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import { HttpClient, HttpClientRequest } from "effect/unstable/http";

/**
 * Result of asking Anthropic about a token.
 *
 * `unknown` is the deliberate catch-all: a proxy, an outage, or a captive
 * network must never sign a working install out of Settings, so only an
 * explicit rejection is treated as one.
 */
export type ClaudeCredentialVerdict = "live" | "rejected" | "unknown";

const ANTHROPIC_MODELS_URL = "https://api.anthropic.com/v1/models?limit=1";
const ANTHROPIC_VERSION_HEADER = "2023-06-01";
const VERIFY_TIMEOUT = Duration.seconds(10);

/**
 * The OAuth token a Claude instance was configured with, if any.
 *
 * Reads the same variable the CLI itself reads, so an instance whose
 * environment carries an `op://` reference is checked with whatever that
 * reference resolved to.
 */
export function claudeOAuthTokenFromEnvironment(
  environment: NodeJS.ProcessEnv,
): string | undefined {
  const token = environment["CLAUDE_CODE_OAUTH_TOKEN"]?.trim();
  return token ? token : undefined;
}

/**
 * Ask Anthropic whether it still accepts `token`.
 *
 * Never fails: transport errors, timeouts, and every non-401 status collapse to
 * `unknown` so the caller can leave the existing status alone.
 */
export const verifyClaudeOAuthToken = Effect.fn("verifyClaudeOAuthToken")(function* (
  token: string,
): Effect.fn.Return<ClaudeCredentialVerdict, never, HttpClient.HttpClient> {
  const client = yield* HttpClient.HttpClient;
  const request = HttpClientRequest.get(ANTHROPIC_MODELS_URL).pipe(
    HttpClientRequest.setHeader("authorization", `Bearer ${token}`),
    HttpClientRequest.setHeader("anthropic-version", ANTHROPIC_VERSION_HEADER),
    HttpClientRequest.setHeader("accept", "application/json"),
  );
  const response = yield* client.execute(request).pipe(
    Effect.timeoutOption(VERIFY_TIMEOUT),
    Effect.orElseSucceed(() => Option.none()),
  );
  if (Option.isNone(response)) {
    return "unknown";
  }
  const { status } = response.value;
  if (status === 401) {
    return "rejected";
  }
  return status >= 200 && status < 300 ? "live" : "unknown";
});
