import { describe, expect, it } from "vite-plus/test";

import { migrationEntries } from "./Migrations.ts";

const RESERVED_FORK_ID_START = 1000;
const GRANDFATHERED_FORK_IDS = new Set([33]);

// Register a migration's name here the moment it is authored for a
// TrogonStack-only feature, so this test can catch it landing outside the
// reserved block instead of that surfacing as a collision with upstream.
const FORK_MIGRATION_NAMES = new Set(["ProjectionThreadParent"]);

describe("migrationEntries", () => {
  it("never assigns the same id twice", () => {
    const ids = migrationEntries.map(([id]) => id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("lists entries in ascending id order", () => {
    const ids = migrationEntries.map(([id]) => id);
    expect(ids).toEqual([...ids].sort((a, b) => a - b));
  });

  it("keeps fork-only migrations out of the shared low-id range", () => {
    for (const [id, name] of migrationEntries) {
      if (!FORK_MIGRATION_NAMES.has(name)) {
        continue;
      }
      expect(id >= RESERVED_FORK_ID_START || GRANDFATHERED_FORK_IDS.has(id)).toBe(true);
    }
  });
});
