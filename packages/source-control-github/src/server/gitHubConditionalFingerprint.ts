import * as Cache from "effect/Cache";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";
import * as Semaphore from "effect/Semaphore";
import { normalizeHost } from "@t3tools/source-control-core/server/pullRequestUtils";
import type { ProviderRepositoryRef } from "@t3tools/source-control-core/server/PullRequestProvider";

import * as GitHubRest from "./GitHubRest.ts";
import * as GitHubRequestPolicy from "./GitHubRequestPolicy.ts";
import type { GitHubPullRequestApiError } from "./GitHubPullRequestApi.ts";
import type { GitHubPullRequestWatchFingerprint } from "./gitHubPullRequestJson.ts";

const PullSchema = Schema.Struct({
  state: Schema.String,
  merged_at: Schema.optional(Schema.NullOr(Schema.String)),
  mergeable: Schema.NullOr(Schema.Boolean),
  mergeable_state: Schema.optional(Schema.String),
  updated_at: Schema.String,
  comments: Schema.Int,
  review_comments: Schema.Int,
  head: Schema.Struct({ sha: Schema.String }),
});
const decodePull = Schema.decodeUnknownOption(Schema.fromJsonString(PullSchema));

const CheckRunsSchema = Schema.Struct({
  check_runs: Schema.Array(
    Schema.Struct({
      status: Schema.String,
      conclusion: Schema.optional(Schema.NullOr(Schema.String)),
    }),
  ),
});
const decodeCheckRuns = Schema.decodeUnknownOption(Schema.fromJsonString(CheckRunsSchema));

const StatusSchema = Schema.Struct({
  statuses: Schema.Array(Schema.Struct({ state: Schema.String })),
});
const decodeStatus = Schema.decodeUnknownOption(Schema.fromJsonString(StatusSchema));

const UNSUPPORTED_STATUSES = new Set([404, 405, 501]);

const countStates = (states: ReadonlyArray<string>) => {
  const counts = new Map<string, number>();
  for (const state of states) counts.set(state, (counts.get(state) ?? 0) + 1);
  return [...counts]
    .map(([state, count]) => `${state}:${count}`)
    .toSorted()
    .join(",");
};

interface Validated {
  readonly etag: string;
  readonly summary: unknown;
}

/**
 * The watch fingerprint from conditional REST reads. A 304 spends no quota, and none of these
 * reads moves for an idle pull request. `fallback` is the GraphQL read, used whenever REST
 * cannot answer: no credential, a host or page size REST does not serve, or an unparseable body.
 */
export const makeConditionalWatchFingerprint = Effect.gen(function* () {
  const api = yield* GitHubRest.GitHubRest;
  const entries = yield* Cache.makeWith(
    (_key: string) =>
      Effect.sync(() => ({
        gate: Semaphore.makeUnsafe(1),
        validators: new Map<string, Validated>(),
        supported: true,
      })),
    { capacity: 128, timeToLive: () => "30 minutes" },
  );

  return (
    input: ProviderRepositoryRef & { readonly number: number },
    fallback: Effect.Effect<GitHubPullRequestWatchFingerprint | null, GitHubPullRequestApiError>,
  ) =>
    Effect.gen(function* () {
      const host = normalizeHost(input.host);
      const held = yield* Effect.option(api.credential(host));
      if (Option.isNone(held)) return yield* fallback;
      const { token, fingerprint } = held.value;
      const entry = yield* Cache.get(
        entries,
        `${fingerprint}\0${host}\0${input.repository}\0${input.number}`,
      );
      return yield* entry.gate.withPermit(
        Effect.gen(function* () {
          if (!entry.supported) return yield* fallback;
          const unsupported = Effect.sync(() => {
            entry.supported = false;
          }).pipe(Effect.andThen(fallback));
          const read = Effect.fn("gitHubConditionalFingerprint.read")(function* <A>(
            path: string,
            summarize: (response: GitHubRest.GitHubRestResponse) => A | null,
          ) {
            const previous = entry.validators.get(path);
            const response = yield* api.rest({
              host,
              operation: "getPullRequestWatchFingerprint",
              path,
              ...(previous === undefined ? {} : { ifNoneMatch: previous.etag }),
            });
            if (response.status === 304 && previous !== undefined) return previous.summary as A;
            const etag = response.headers["etag"]?.trim();
            const summary =
              response.status === 200 &&
              !response.truncated &&
              // Another page may hold runs this read cannot see.
              !/rel="next"/.test(response.headers["link"] ?? "")
                ? summarize(response)
                : null;
            if (summary === null || !etag) {
              entry.validators.delete(path);
              return null;
            }
            entry.validators.set(path, { etag, summary });
            return summary;
          });
          const root = `repos/${input.repository}`;
          const pull = yield* read(`${root}/pulls/${input.number}`, (response) => {
            const decoded = decodePull(response.body);
            if (Option.isNone(decoded)) return null;
            const pr = decoded.value;
            return {
              sha: pr.head.sha,
              status: [
                pr.merged_at ? "MERGED" : pr.state.toUpperCase(),
                pr.mergeable === null ? "UNKNOWN" : pr.mergeable ? "MERGEABLE" : "CONFLICTING",
                pr.mergeable_state ?? "",
                pr.head.sha,
              ].join(" "),
              remarks: [pr.comments, pr.review_comments, pr.updated_at].join(" "),
            };
          });
          if (pull === null) return yield* unsupported;
          const { sha } = pull;
          const [runs, statuses] = yield* Effect.all(
            [
              read(`${root}/commits/${sha}/check-runs?filter=all&per_page=100`, (response) => {
                const decoded = decodeCheckRuns(response.body);
                return Option.isNone(decoded)
                  ? null
                  : countStates(
                      decoded.value.check_runs.map((run) =>
                        run.status === "completed" ? (run.conclusion ?? "completed") : run.status,
                      ),
                    );
              }),
              read(`${root}/commits/${sha}/status?per_page=100`, (response) => {
                const decoded = decodeStatus(response.body);
                return Option.isNone(decoded)
                  ? null
                  : countStates(decoded.value.statuses.map((status) => status.state));
              }),
            ],
            { concurrency: 2 },
          );
          if (runs === null || statuses === null) return yield* unsupported;
          return {
            status: [pull.status, runs, statuses].join(" "),
            remarks: pull.remarks,
            conditional: true,
          } satisfies GitHubPullRequestWatchFingerprint;
        }).pipe(
          Effect.provideService(GitHubRequestPolicy.PinnedGitHubCredential, {
            host,
            token,
            credentialFingerprint: fingerprint,
          }),
          Effect.catchTags({
            GitHubApiNotFoundError: () =>
              Effect.sync(() => {
                entry.supported = false;
              }).pipe(Effect.andThen(fallback)),
            GitHubApiResponseError: (error) =>
              Effect.sync(() => {
                entry.supported &&= !UNSUPPORTED_STATUSES.has(error.status);
              }).pipe(Effect.andThen(fallback)),
          }),
        ),
      );
    });
});
