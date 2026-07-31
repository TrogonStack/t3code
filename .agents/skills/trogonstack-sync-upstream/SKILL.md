---
name: trogonstack-sync-upstream
description: Merge the latest pingdotgg/t3code upstream main into this TrogonStack fork while preserving the fork's tracked divergences, fork-only migrations, and merge-based sync history. Use when asked to sync, upgrade, update, or catch up the fork with upstream, or to pull in the latest upstream changes.
---

# Sync with upstream

This fork tracks [pingdotgg/t3code](https://github.com/pingdotgg/t3code) as the `upstream` remote and carries a small set of intentional divergences. Syncing is always a **merge**, never a rebase: rebasing would rewrite fork history and break the shared migration tracker and published PR references.

## 1. Prepare

1. Start from a clean working tree on `main` with `origin/main` up to date.
2. Fetch upstream: `git fetch upstream`.
3. If `git log --oneline main..upstream/main` is empty, the fork is already current; stop and report that.
4. Load the divergence context before touching any conflict:
   - Read `docs/fork/README.md` and every active ledger entry in `docs/fork/`. These are the features the merge must not regress.
   - List fork-only commits: `git log --oneline upstream/main..main`.

## 2. Create the sync branch

```
git switch -c yordis/chore-sync-upstream-{YYYYMMDD}-{NUM} main
git merge upstream/main
```

Use today's date for `{YYYYMMDD}` and a sequence number for `{NUM}`, starting at `1` and incrementing for each additional sync the same day (e.g. `yordis/chore-sync-upstream-20260729-1`). Check `git branch --list --all 'yordis/chore-sync-upstream-*'` to pick the next free number.

## 3. Resolve conflicts

Default to upstream's shape and re-apply the fork's intent on top of it. Concretely:

- If the conflicting area belongs to a ledger divergence, keep the fork behavior and adapt it to upstream's new structure. Do not silently drop a divergence.
- If the fork's version of something is a superset of what upstream added, keep the fork's version and skip the redundant upstream piece (this happened in PR #9 with bootstrap-dispatch logic already covered by the fork's `ThreadBootstrapService`).
- If upstream merged an equivalent of a fork divergence, take upstream's version and delete the corresponding ledger entry outright (numbers are never reused; git history is the record).
- Everything else: take upstream.
- For conflicts under `.repos/` (vendored subtrees), do not hand-merge; take either side to complete the merge, then regenerate with `vpr sync:repos` (or `vpr sync:repos --repo <id>`) so `.repos/` matches the installed dependency versions.

Record every nontrivial resolution decision as you go; each one becomes a PR body bullet.

### Migration rules (apps/server/src/persistence)

- Upstream migrations sync as-is, keeping their own ids. `Migrations.ts` and every file under `apps/server/src/persistence/Migrations/` (excluding `fork/`) must stay byte-identical to upstream. If either conflicts during a sync, take upstream's side wholesale rather than hand-merging.
- Fork-only schema never lands in `Migrations.ts`. It lives only in `apps/server/src/persistence/ForkMigrations.ts`, numbered independently under `apps/server/src/persistence/Migrations/fork/`, and tracked in its own `trogonstack_fork_migrations` ledger table instead of the shared `effect_sql_migrations` table.
- Because the two chains never share a ledger, id collisions between fork and upstream migrations are structurally impossible - there is nothing to renumber or offset during a sync.
- After resolving `Migrations.ts`, run the fail-fast guard: `vp test run apps/server/src/persistence/ForkMigrations.test.ts`. It rejects any fork migration name that leaks into the shared `migrationEntries` chain.

## 4. Verify

A sync's blast radius is the whole repo, so this is the one workflow where full-suite local verification is expected (matching prior sync PRs):

```
vp run -r --concurrency-limit 2 typecheck
vp run -r test
vp lint --report-unused-disable-directives
```

Fix fallout on the sync branch before opening the PR. If upstream bumped a dependency that has a vendored subtree, confirm `.repos/` was synced in the same change.

## 5. Open and merge the PR

1. Commit with DCO sign-off (`git commit -s`). Conflict resolutions belong in the merge commit; follow-up fixes go in separate conventional commits on the branch.
2. Push the branch and open a PR against `main`:
   - Title: `chore: sync with upstream pingdotgg/t3code main`
   - Body: bullets only, each explaining the why of a nontrivial resolution decision (what was kept, dropped, or renumbered, and why). No file lists, no test plan section.
3. Merge the PR with a **merge commit**, never squash. Squashing would drop the `upstream/main` merge parent, so the next sync would re-conflict on everything already resolved.
4. Share the PR link, then delete the sync branch.
