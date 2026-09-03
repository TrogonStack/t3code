import { assert, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as SqlClient from "effect/unstable/sql/SqlClient";
import { describe, expect, it as vpIt } from "vite-plus/test";

import { migrationEntries, runMigrations } from "./Migrations.ts";
import {
  forkMigrationEntries,
  realignSharedMigrationLedger,
  runForkMigrations,
  SharedMigrationLedgerMismatchError,
  verifySharedMigrationLedger,
} from "./ForkMigrations.ts";
import * as NodeSqliteClient from "@t3tools/shared/nodeSqliteClient";

const UPSTREAM_MAX = Math.max(...migrationEntries.map(([id]) => id));

// Highest upstream id that existed while the fork still shipped
// ProjectionThreadParent inside the shared chain. A legacy install can only
// have reached this far, so the simulations below stop here before shifting
// the ledger; running the modern chain first would leave real rows sitting on
// the ids the shift moves into.
const LEGACY_UPSTREAM_MAX = 35;

const selectSharedLedger = (sql: SqlClient.SqlClient) =>
  sql<{ readonly migration_id: number; readonly name: string }>`
    SELECT migration_id, name FROM effect_sql_migrations ORDER BY migration_id
  `;

const selectForkLedger = (sql: SqlClient.SqlClient) =>
  sql<{ readonly migration_id: number; readonly name: string }>`
    SELECT migration_id, name FROM trogonstack_fork_migrations ORDER BY migration_id
  `;

describe("forkMigrationEntries", () => {
  vpIt("never overlaps with the shared migrationEntries names", () => {
    const sharedNames: ReadonlySet<string> = new Set(migrationEntries.map(([, name]) => name));
    for (const [, name] of forkMigrationEntries) {
      expect(sharedNames.has(name)).toBe(false);
    }
  });

  vpIt("lists entries with unique, ascending ids", () => {
    const ids = forkMigrationEntries.map(([id]) => id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toEqual([...ids].sort((a, b) => a - b));
  });
});

const layer = it.layer(Layer.mergeAll(NodeSqliteClient.layerMemory()));

layer("fresh install", (it) => {
  it.effect("runs the fork chain on its own ledger without touching shared numbering", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;

      yield* realignSharedMigrationLedger();
      yield* verifySharedMigrationLedger();
      yield* runMigrations();
      yield* runForkMigrations();

      const columns = yield* sql<{ readonly name: string }>`
        PRAGMA table_info(projection_threads)
      `;
      assert.ok(columns.some((column) => column.name === "parent_thread_id"));

      const forkLedger = yield* selectForkLedger(sql);
      assert.deepStrictEqual(forkLedger, [{ migration_id: 1, name: "ProjectionThreadParent" }]);

      const sharedLedger = yield* selectSharedLedger(sql);
      assert.strictEqual(sharedLedger[sharedLedger.length - 1]?.migration_id, UPSTREAM_MAX);
      assert.ok(!sharedLedger.some((row) => row.name === "ProjectionThreadParent"));
    }),
  );
});

// Each layer() block gets its own in-memory database, so every ledger scenario
// needs its own block rather than sharing one with the others.
layer("verifySharedMigrationLedger on a fresh database", (it) => {
  it.effect("passes when the ledger table does not exist yet", () =>
    Effect.gen(function* () {
      yield* verifySharedMigrationLedger();
    }),
  );
});

layer("verifySharedMigrationLedger after the shared chain runs", (it) => {
  it.effect("passes, and keeps passing with ids beyond this build's chain", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;

      yield* runMigrations();
      yield* runForkMigrations();
      yield* verifySharedMigrationLedger();

      yield* sql`
        INSERT INTO effect_sql_migrations (migration_id, name, created_at)
        VALUES (${UPSTREAM_MAX + 1}, 'SomeFutureMigration', CURRENT_TIMESTAMP)
      `;

      yield* verifySharedMigrationLedger();
    }),
  );
});

layer("verifySharedMigrationLedger on a partially migrated database", (it) => {
  it.effect("passes when the ledger stops partway through the chain", () =>
    Effect.gen(function* () {
      yield* runMigrations({ toMigrationInclusive: 20 });
      yield* verifySharedMigrationLedger();
    }),
  );
});

layer("verifySharedMigrationLedger with a divergent branch's chain", (it) => {
  it.effect("fails, naming the migrations that would be skipped", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;

      // Reproduces ~/.t3/dev/state.sqlite: the shared chain ran through 32, then
      // a branch that numbered its OrchestrationV2 chain at 33+ took over, so
      // this build's 33.. never ran and their columns are missing.
      yield* runMigrations({ toMigrationInclusive: 32 });
      const divergent = ["OrchestrationV2", "OrchestrationV2Subagents", "ScheduledTasks"];
      for (const [index, name] of divergent.entries()) {
        yield* sql`
          INSERT INTO effect_sql_migrations (migration_id, name, created_at)
          VALUES (${33 + index}, ${name}, CURRENT_TIMESTAMP)
        `;
      }

      const error = yield* Effect.flip(verifySharedMigrationLedger());

      assert.instanceOf(error, SharedMigrationLedgerMismatchError);
      assert.strictEqual(error.latestLedgerId, 35);
      assert.deepStrictEqual(
        error.skipped.map(({ id }) => id),
        [33, 34, 35],
      );
      assert.deepStrictEqual(error.skipped[0], {
        id: 33,
        expected: "ProjectionThreadsSettled",
        recorded: "OrchestrationV2",
      });
      assert.include(error.message, "ProjectionThreadsSettled");
      assert.include(error.message, "OrchestrationV2");
      assert.include(error.message, "T3CODE_HOME");
    }),
  );
});

layer("verifySharedMigrationLedger with a gap in the ledger", (it) => {
  it.effect("reports the missing id as a skipped migration", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;

      yield* runMigrations({ toMigrationInclusive: 20 });
      yield* sql`DELETE FROM effect_sql_migrations WHERE migration_id = 15`;

      const error = yield* Effect.flip(verifySharedMigrationLedger());

      assert.instanceOf(error, SharedMigrationLedgerMismatchError);
      assert.deepStrictEqual(error.skipped, [
        { id: 15, expected: "ProjectionTurnsSourceProposedPlan", recorded: null },
      ]);
      assert.include(error.message, "no row");
    }),
  );
});

layer("legacy full install", (it) => {
  it.effect("realigns a ledger that ran the old fork chain through upstream 35 (legacy 36)", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;

      yield* runMigrations({ toMigrationInclusive: LEGACY_UPSTREAM_MAX });

      yield* sql`
        ALTER TABLE projection_threads
        ADD COLUMN parent_thread_id TEXT
      `;
      yield* sql`
        CREATE INDEX IF NOT EXISTS idx_projection_threads_parent
        ON projection_threads(parent_thread_id)
      `;

      yield* sql`UPDATE effect_sql_migrations SET migration_id = 36 WHERE migration_id = 35`;
      yield* sql`UPDATE effect_sql_migrations SET migration_id = 35 WHERE migration_id = 34`;
      yield* sql`UPDATE effect_sql_migrations SET migration_id = 34 WHERE migration_id = 33`;
      yield* sql`
        INSERT INTO effect_sql_migrations (migration_id, name, created_at)
        VALUES (33, 'ProjectionThreadParent', CURRENT_TIMESTAMP)
      `;

      yield* realignSharedMigrationLedger();
      yield* verifySharedMigrationLedger();
      yield* runMigrations();
      yield* runForkMigrations();

      const sharedLedger = yield* selectSharedLedger(sql);
      assert.deepStrictEqual(
        sharedLedger.map((row) => row.migration_id),
        Array.from({ length: UPSTREAM_MAX }, (_, index) => index + 1),
      );
      assert.deepStrictEqual(
        sharedLedger.find((row) => row.migration_id === 33)?.name,
        "ProjectionThreadsSettled",
      );
      assert.deepStrictEqual(
        sharedLedger.find((row) => row.migration_id === 34)?.name,
        "ProjectionThreadsSnoozed",
      );
      assert.deepStrictEqual(
        sharedLedger.find((row) => row.migration_id === 35)?.name,
        "ProjectionThreadTitleRegeneration",
      );
      assert.ok(!sharedLedger.some((row) => row.name === "ProjectionThreadParent"));

      const forkLedger = yield* selectForkLedger(sql);
      assert.deepStrictEqual(forkLedger, [{ migration_id: 1, name: "ProjectionThreadParent" }]);

      const columns = yield* sql<{ readonly name: string }>`
        PRAGMA table_info(projection_threads)
      `;
      assert.ok(columns.some((column) => column.name === "parent_thread_id"));

      const ledgerBeforeSecondRealign = yield* selectSharedLedger(sql);
      yield* realignSharedMigrationLedger();
      const ledgerAfterSecondRealign = yield* selectSharedLedger(sql);
      assert.deepStrictEqual(ledgerAfterSecondRealign, ledgerBeforeSecondRealign);
    }),
  );
});

layer("legacy mid-history install", (it) => {
  it.effect("realigns a ledger that stopped right after the old fork migration 33", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;

      yield* runMigrations({ toMigrationInclusive: 32 });

      yield* sql`
        ALTER TABLE projection_threads
        ADD COLUMN parent_thread_id TEXT
      `;
      yield* sql`
        CREATE INDEX IF NOT EXISTS idx_projection_threads_parent
        ON projection_threads(parent_thread_id)
      `;
      yield* sql`
        INSERT INTO effect_sql_migrations (migration_id, name, created_at)
        VALUES (33, 'ProjectionThreadParent', CURRENT_TIMESTAMP)
      `;

      yield* realignSharedMigrationLedger();
      yield* verifySharedMigrationLedger();
      yield* runMigrations();
      yield* runForkMigrations();

      const sharedLedger = yield* selectSharedLedger(sql);
      assert.strictEqual(sharedLedger[sharedLedger.length - 1]?.migration_id, UPSTREAM_MAX);

      const columns = yield* sql<{ readonly name: string }>`
        PRAGMA table_info(projection_threads)
      `;
      assert.ok(columns.some((column) => column.name === "title_regeneration_request_id"));

      const forkLedger = yield* selectForkLedger(sql);
      assert.deepStrictEqual(forkLedger, [{ migration_id: 1, name: "ProjectionThreadParent" }]);
    }),
  );
});
