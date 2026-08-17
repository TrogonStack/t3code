import * as Effect from "effect/Effect";
import * as SqlClient from "effect/unstable/sql/SqlClient";

export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  const columns = yield* sql<{ readonly name: string }>`
    PRAGMA table_info(projection_threads)
  `;
  const existing = new Set(columns.map((column) => column.name));

  if (!existing.has("forked_from_thread_id")) {
    yield* sql`ALTER TABLE projection_threads ADD COLUMN forked_from_thread_id TEXT`;
  }
  if (!existing.has("forked_up_to_message_id")) {
    yield* sql`ALTER TABLE projection_threads ADD COLUMN forked_up_to_message_id TEXT`;
  }
  if (!existing.has("fork_mode")) {
    yield* sql`ALTER TABLE projection_threads ADD COLUMN fork_mode TEXT`;
  }
  if (!existing.has("pending_fork_context")) {
    yield* sql`ALTER TABLE projection_threads ADD COLUMN pending_fork_context TEXT`;
  }

  yield* sql`
    CREATE INDEX IF NOT EXISTS idx_projection_threads_forked_from
    ON projection_threads(forked_from_thread_id)
  `;
});
