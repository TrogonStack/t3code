import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as TestClock from "effect/testing/TestClock";

import * as GitHubGraphQlBudget from "./githubGraphQlBudget.ts";

const RESET_AT = "2026-08-13T14:00:00.000Z";
const NEXT_RESET_AT = "2026-08-13T15:00:00.000Z";
const BEFORE_RESET = Date.parse("2026-08-13T13:30:00.000Z");
const AFTER_RESET = Date.parse("2026-08-13T14:00:01.000Z");

function rateLimit(remaining: number, limit = 5_000, resetAt = RESET_AT): string {
  return JSON.stringify({
    data: {
      viewer: { login: "bilal" },
      rateLimit: { cost: 14, limit, remaining, resetAt },
    },
  });
}

describe("GitHub GraphQL budget", () => {
  it.effect("adds rate metadata to a read query", () =>
    Effect.gen(function* () {
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;

      const query = yield* budget.query(
        "github.com",
        'query { repository(owner: "acme", name: "web") { name } }',
      );

      expect(query).toContain("rateLimit { cost limit remaining resetAt }");
      expect(query).toContain('repository(owner: "acme", name: "web") { name }');
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("protects the last ten percent with the shared provider cooldown error", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(500));

      const error = yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));
      expect(error).toMatchObject({
        _tag: "SourceControlRateLimitPausedError",
        provider: "github",
        host: "github.com",
        retryAt: Date.parse(RESET_AT),
      });
      expect(error.message).toBe(
        "github requests to github.com are paused until the rate limit resets.",
      );

      yield* TestClock.setTime(AFTER_RESET);
      expect(yield* budget.query("github.com", "query { viewer { login } }")).toContain(
        "rateLimit",
      );
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("keeps hosts isolated", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(0));

      const error = yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));
      expect(error._tag).toBe("SourceControlRateLimitPausedError");
      expect(yield* budget.query("github.example.com", "query { viewer { login } }")).toContain(
        "rateLimit",
      );
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("keeps the lower remaining value from out-of-order responses", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(400));
      yield* budget.observe("github.com", rateLimit(600));

      const error = yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));
      expect(error).toMatchObject({
        _tag: "SourceControlRateLimitPausedError",
        retryAt: Date.parse(RESET_AT),
      });
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("ignores a response from an older reset window", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(400, 5_000, NEXT_RESET_AT));
      yield* budget.observe("github.com", rateLimit(1_000, 5_000, RESET_AT));

      const error = yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));
      expect(error).toMatchObject({
        _tag: "SourceControlRateLimitPausedError",
        retryAt: Date.parse(NEXT_RESET_AT),
      });
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("accepts a response from a later reset window", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(1_000));
      yield* budget.observe("github.com", rateLimit(400, 5_000, NEXT_RESET_AT));

      const error = yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));
      expect(error).toMatchObject({
        _tag: "SourceControlRateLimitPausedError",
        retryAt: Date.parse(NEXT_RESET_AT),
      });
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("allows reads above the reserve", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(515));

      expect(yield* budget.query("github.com", "query { viewer { login } }")).toContain(
        "rateLimit",
      );
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("allows an interactive read to use the reserve", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(500));

      expect(
        yield* budget.query("github.com", "query { viewer { login } }", {
          allowReserve: true,
        }),
      ).toContain("rateLimit");
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("reserves an admitted query cost before its response", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(514));

      expect(yield* budget.query("github.com", "query { viewer { login } }")).toContain(
        "rateLimit",
      );
      const error = yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));

      expect(error).toMatchObject({
        _tag: "SourceControlRateLimitPausedError",
        retryAt: Date.parse(RESET_AT),
      });
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("ignores malformed or partial rate metadata", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", "{");
      yield* budget.observe(
        "github.com",
        '{"data":{"rateLimit":{"limit":0,"remaining":-1,"resetAt":"never"}}}',
      );

      expect(yield* budget.query("github.com", "query { viewer { login } }")).toContain(
        "rateLimit",
      );
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("does not add a read field to a mutation", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      const mutation = "mutation { addComment(input: {}) { clientMutationId } }";
      yield* budget.observe("github.com", rateLimit(0));

      expect(yield* budget.query("github.com", mutation)).toBe(mutation);
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("charges a write for the batch it carries, since it cannot report its own cost", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      // Twenty points above the reserve, which is exactly what the mutation below spends.
      yield* budget.observe("github.com", rateLimit(520));

      yield* budget.query("github.com", "mutation { f0: markFileAsViewed { id } }", {
        estimatedCost: 20,
      });

      const error = yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));
      expect(error).toMatchObject({ _tag: "SourceControlRateLimitPausedError" });
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("takes the host's own number over a write's guess, however high the guess was", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(4_000));
      // A batch charged far more than it really spent would otherwise hold reads until the reset.
      yield* budget.query("github.com", "mutation { f0: markFileAsViewed { id } }", {
        estimatedCost: 3_900,
      });
      yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));

      yield* budget.observe("github.com", rateLimit(3_990));

      expect(yield* budget.query("github.com", "query { viewer { login } }")).toContain(
        "rateLimit",
      );
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("still ignores an out-of-order answer once the guess has been settled", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(600));
      yield* budget.query("github.com", "mutation { f0: markFileAsViewed { id } }", {
        estimatedCost: 50,
      });
      // The host's own number settles the guess, and the answer behind it is stale again.
      yield* budget.observe("github.com", rateLimit(513));
      yield* budget.observe("github.com", rateLimit(600));

      const error = yield* Effect.flip(budget.query("github.com", "query { viewer { login } }"));
      expect(error).toMatchObject({ _tag: "SourceControlRateLimitPausedError" });
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );

  it.effect("lets a write through even with nothing left, rather than holding a press back", () =>
    Effect.gen(function* () {
      yield* TestClock.setTime(BEFORE_RESET);
      const budget = yield* GitHubGraphQlBudget.GitHubGraphQlBudget;
      yield* budget.observe("github.com", rateLimit(0));

      const mutation = "mutation { f0: markFileAsViewed { id } }";
      expect(yield* budget.query("github.com", mutation, { estimatedCost: 40 })).toBe(mutation);
    }).pipe(Effect.provide(GitHubGraphQlBudget.layer)),
  );
});
