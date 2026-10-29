import { assert, describe, it } from "@effect/vitest";
import * as Crypto from "effect/Crypto";
import * as Effect from "effect/Effect";

import * as Orchestrator from "./Orchestrator.ts";
import * as ProjectionStore from "./ProjectionStore.ts";
import * as PullRequestService from "../pullRequest/PullRequestService.ts";
import * as PullRequestWatchReactor from "./PullRequestWatchReactor.ts";

const STARTED = "2026-10-02T12:00:00.000Z";

const testCrypto = Crypto.make({
  randomBytes: (size) => new Uint8Array(size).fill(1),
  digest: (_algorithm, data) => Effect.succeed(data),
});

describe("PullRequestWatchReactor", () => {
  it.effect("asks the host again before reading when there is no fingerprint to compare", () =>
    Effect.gen(function* () {
      const calls: Array<string> = [];
      const link = {
        host: "github.com",
        repository: "o/r",
        number: 1,
        url: "https://github.com/o/r/pull/1",
        snapshot: null,
        watch: {
          startedAt: STARTED,
          headSha: "aaaaaaaaaa",
          failedChecks: [],
          passed: false,
          passedChecks: [],
          remarksThrough: STARTED,
          remarkIds: [],
          conflicting: false,
          wakes: 0,
        },
      };
      const thread = {
        id: "thread-1",
        projectId: "project-1",
        lineage: { rootThreadId: "thread-1", parentThreadId: null, relationshipToParent: null },
        settledOverride: null,
        settledAt: null,
        pullRequests: [link],
      };
      const reactor = yield* PullRequestWatchReactor.make.pipe(
        Effect.provideService(Orchestrator.OrchestratorV2, {
          dispatch: () => Effect.void,
        } as never),
        Effect.provideService(ProjectionStore.ProjectionStoreV2, {
          getThreadsWithPullRequests: () => Effect.succeed([thread]),
        } as never),
        Effect.provideService(PullRequestService.PullRequestService, {
          watchFingerprint: () => Effect.succeed(null),
          invalidate: () => Effect.sync(() => void calls.push("invalidate")),
          detail: () =>
            Effect.sync(() => {
              calls.push("detail");
              return {
                state: "open",
                headSha: "aaaaaaaaaa",
                baseBranch: "main",
                checks: [],
                mergeability: "mergeable",
                capabilities: { checksAlwaysReport: false },
              };
            }),
          activity: () =>
            Effect.succeed({ comments: [], reviewThreads: [], commentsTruncated: false }),
        } as never),
        Effect.provideService(Crypto.Crypto, testCrypto),
      );
      yield* reactor.sweep;
      assert.deepEqual(calls, ["invalidate", "detail"]);
    }),
  );
});
