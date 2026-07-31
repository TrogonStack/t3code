/**
 * ForkMigrationsLive - fork-only migration runner with its own ledger
 *
 * Fork-only schema (TrogonStack additions that do not exist upstream) lives
 * in this second migration chain, tracked in its own migrations table
 * (`trogonstack_fork_migrations`) instead of the shared `effect_sql_migrations`
 * ledger. This keeps `Migrations.ts` and every file under `./Migrations/`
 * byte-identical to upstream (pingdotgg/t3code) forever, since the shared
 * chain never has to make room for fork ids. A fork migration must never be
 * added to `Migrations.ts` - new fork-only schema belongs here, under
 * `./Migrations/fork/`, numbered independently of the shared chain.
 *
 * The fork once shipped its first migration (ProjectionThreadParent) as id 33
 * in the shared chain, before this second chain existed, which permanently
 * offset every upstream id after it. `realignSharedMigrationLedger` repairs
 * that on installs that ran the old shared-chain history, moving the shared
 * ledger back to upstream's numbering before the shared migrator runs.
 */

import * as Migrator from "effect/unstable/sql/Migrator";
import * as Layer from "effect/Layer";
import * as Effect from "effect/Effect";
import * as SqlClient from "effect/unstable/sql/SqlClient";

import Migration0001 from "./Migrations/fork/001_ProjectionThreadParent.ts";

export const FORK_MIGRATIONS_TABLE = "trogonstack_fork_migrations";

export const forkMigrationEntries = [[1, "ProjectionThreadParent", Migration0001]] as const;

export const makeForkMigrationLoader = (throughId?: number) =>
  Migrator.fromRecord(
    Object.fromEntries(
      forkMigrationEntries
        .filter(([id]) => throughId === undefined || id <= throughId)
        .map(([id, name, migration]) => [`${id}_${name}`, migration]),
    ),
  );

const run = Migrator.make({});

export interface RunForkMigrationsOptions {
  readonly toMigrationInclusive?: number | undefined;
}

/**
 * Run all pending fork-only migrations against the fork's own ledger table.
 *
 * @returns Effect containing array of executed migrations
 */
export const runForkMigrations = Effect.fn("runForkMigrations")(function* ({
  toMigrationInclusive,
}: RunForkMigrationsOptions = {}) {
  const executedMigrations = yield* run({
    loader: makeForkMigrationLoader(toMigrationInclusive),
    table: FORK_MIGRATIONS_TABLE,
  });
  const migrations = executedMigrations.map(([id, name]) => `${id}_${name}`);
  yield* migrations.length === 0
    ? Effect.logDebug("Fork database schema is current")
    : Effect.log("Fork migrations ran successfully").pipe(Effect.annotateLogs({ migrations }));
  return executedMigrations;
});

/**
 * Realign the shared `effect_sql_migrations` ledger for installs that ran
 * the fork's old migration chain, where ProjectionThreadParent shipped as
 * shared id 33 and pushed upstream's 33/34/35 to 34/35/36.
 *
 * No-ops for fresh installs (no shared ledger table yet) and for installs
 * already realigned (no id-33 ProjectionThreadParent row left to move).
 *
 * TODO: delete this function, its call in Layers/Sqlite.ts, and the legacy
 * scenarios in ForkMigrations.test.ts once every install that ran the old
 * shared-chain fork history has started up with it at least once. Fresh
 * installs never need it.
 */
export const realignSharedMigrationLedger = Effect.fn("realignSharedMigrationLedger")(function* () {
  const sql = yield* SqlClient.SqlClient;

  const tables = yield* sql<{ readonly name: string }>`
      SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'effect_sql_migrations'
    `;
  if (tables.length === 0) {
    return;
  }

  const legacyRows = yield* sql<{ readonly migration_id: number; readonly name: string }>`
      SELECT migration_id, name FROM effect_sql_migrations
      WHERE migration_id = 33 AND name = 'ProjectionThreadParent'
    `;
  if (legacyRows.length === 0) {
    return;
  }

  yield* sql.withTransaction(
    Effect.gen(function* () {
      yield* sql`
          DELETE FROM effect_sql_migrations WHERE migration_id = 33 AND name = 'ProjectionThreadParent'
        `;
      yield* sql`UPDATE effect_sql_migrations SET migration_id = 33 WHERE migration_id = 34`;
      yield* sql`UPDATE effect_sql_migrations SET migration_id = 34 WHERE migration_id = 35`;
      yield* sql`UPDATE effect_sql_migrations SET migration_id = 35 WHERE migration_id = 36`;
    }),
  );

  yield* Effect.log("Realigned shared migration ledger to upstream numbering");
});

/**
 * Layer that runs fork migrations when the layer is built.
 */
export const ForkMigrationsLive = Layer.effectDiscard(runForkMigrations());
