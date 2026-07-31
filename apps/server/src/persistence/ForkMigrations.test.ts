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
} from "./ForkMigrations.ts";
import * as NodeSqliteClient from "./NodeSqliteClient.ts";

const UPSTREAM_MAX = Math.max(...migrationEntries.map(([id]) => id));

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

layer("legacy full install", (it) => {
  it.effect("realigns a ledger that ran the old fork chain through upstream 35 (legacy 36)", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;

      yield* runMigrations();

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
