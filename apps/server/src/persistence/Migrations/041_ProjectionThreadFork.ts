import * as Effect from "effect/Effect";
import * as SqlClient from "effect/unstable/sql/SqlClient";

export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;

  yield* sql`
    ALTER TABLE projection_threads
    ADD COLUMN forked_from_thread_id TEXT
  `;

  yield* sql`
    ALTER TABLE projection_threads
    ADD COLUMN forked_up_to_message_id TEXT
  `;

  yield* sql`
    ALTER TABLE projection_threads
    ADD COLUMN fork_mode TEXT
  `;

  yield* sql`
    ALTER TABLE projection_threads
    ADD COLUMN pending_fork_context TEXT
  `;

  yield* sql`
    CREATE INDEX IF NOT EXISTS idx_projection_threads_forked_from
    ON projection_threads(forked_from_thread_id)
  `;
});
