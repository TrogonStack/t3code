# 0006: Fork schema on its own migration ledger

- PR: [TrogonStack/t3code#13](https://github.com/TrogonStack/t3code/pull/13),
  [TrogonStack/t3code#16](https://github.com/TrogonStack/t3code/pull/16)
- Status: active

## What you can do now

- Fork-only schema lands on a migration chain of its own, so upstream's
  migration numbering is never taken by the fork and a sync no longer has to
  make room for fork ids.
- An install that ran the fork's older history is brought back onto upstream
  numbering on the next start, with no manual database surgery.
- Starting a server against a database that some other branch migrated stops
  right away and names every migration that would have been skipped, instead
  of booting on a schema that only looks current and then failing in an
  unrelated query.

## Why

The fork carries schema upstream does not have. Numbering it inside the
shared chain permanently offsets every upstream id after it, which turns each
sync into a conflict over files that should never diverge.

The startup check exists because one T3 home routinely gets shared across
branches here. Migrations are chosen by id alone, so a home that another
branch already migrated makes this build skip its own migrations while the
ledger still reads as fully migrated. That failure then surfaces far from its
cause, as a query error and a restart loop, which is an expensive way to
learn that the home directory belongs to somewhere else.

## Upstream considerations

The second chain only exists because the fork carries its own schema, so it
is not something upstream wants as it stands, and it is deliberately built so
the shared migration files stay byte-identical to upstream forever. New
fork-only schema belongs on the fork chain, never on the shared one.

The startup check is not fork-specific and upstream may want it on its own
merits, since anyone running several checkouts against one home can hit the
same silent skip.
