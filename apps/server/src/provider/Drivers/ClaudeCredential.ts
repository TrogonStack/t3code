/**
 * Liveness check for a configured Claude OAuth token via one cheap `GET /v1/models`: the probe
 * only reports which credential was found. Only Anthropic-shaped values are sent, so a
 * placeholder never causes a misleading 401.
 * @module provider/Drivers/ClaudeCredential
 */
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import { HttpClient, HttpClientRequest } from "effect/http";

/**
 * Result of asking Anthropic about a token.
 *
 * `unknown` is the deliberate catch-all: a proxy, an outage, or a captive
 * network must never sign a working install out of Settings, so only an
 * explicit rejection is treated as one.
 */
export type ClaudeCredentialVerdict = "live" | "rejected" | "unknown";

const ANTHROPIC_MODELS_URL = "https://api.anthropic.com/v1/models?limit=1";
/**
 * `sk-ant-` covers setup tokens (`sk-ant-oat01-`) and API keys (`sk-ant-api03-`). The tail is
 * spelled out so placeholders like `sk-ant-oat01-${MY_TOKEN}` are not treated as credentials.
 */
const ANTHROPIC_CREDENTIAL = /^sk-ant-[A-Za-z0-9._~+/=-]+$/;
const ANTHROPIC_VERSION_HEADER = "2023-06-01";
const VERIFY_TIMEOUT = Duration.seconds(10);

/**
 * The OAuth token a Claude instance was configured with, if any.
 *
 * Reads the same variable the CLI itself reads, so an instance whose
 * environment reads the token from a secret store is checked with whatever
 * that secret resolved to.
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
 * Never fails: a value that is not an Anthropic credential, transport errors,
 * timeouts, and every non-401 status all collapse to `unknown` so the caller
 * can leave the existing status alone.
 */
export const verifyClaudeOAuthToken = Effect.fn("verifyClaudeOAuthToken")(function* (
  token: string,
): Effect.fn.Return<ClaudeCredentialVerdict, never, HttpClient.HttpClient> {
  if (!ANTHROPIC_CREDENTIAL.test(token)) {
    return "unknown";
  }
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
