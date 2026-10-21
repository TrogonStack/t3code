---
name: trogonstack-sync-upstream
description: Merge the latest pingdotgg/t3code upstream main into this TrogonStack fork while preserving the fork's tracked divergences and merge-based sync history. Use when asked to sync, upgrade, update, or catch up the fork with upstream, or to pull in the latest upstream changes.
---

# Sync with upstream

This fork tracks [pingdotgg/t3code](https://github.com/pingdotgg/t3code) as the `upstream` remote and carries a small set of intentional divergences. Syncing is always a **merge**, never a rebase: rebasing would rewrite fork history and break published PR references.

## 1. Prepare

Every sync runs in its own linked worktree, never in the main checkout. The main checkout is often serving the developer's live T3 Code session and carries their uncommitted work; a sync installs dependencies, rewrites the lockfile, and runs the full suite, none of which should touch that.

1. Fetch both remotes: `git fetch origin && git fetch upstream`. Do not switch branches, stash, or clean anything in the main checkout.
2. If `git log --oneline origin/main..upstream/main` is empty, the fork is already current; stop and report that.
3. Load the divergence context before touching any conflict:
   - Read `docs/fork/README.md` and every active ledger entry in `docs/fork/`. These are the features the merge must not regress.
   - List fork-only commits: `git log --oneline upstream/main..origin/main`.

## 2. Create the sync branch

Create the branch in a sibling worktree based on `origin/main`, install dependencies there, then merge:

```
git worktree add -b yordis/chore-sync-upstream-{YYYYMMDD}-{NUM} ../t3code-wt-sync-upstream-{YYYYMMDD}-{NUM} origin/main
cd ../t3code-wt-sync-upstream-{YYYYMMDD}-{NUM}
T3CODE_PROJECT_ROOT="$(git worktree list --porcelain | head -1 | cut -d' ' -f2)" node scripts/setup-worktree.ts
git merge upstream/main
```

Use today's date for `{YYYYMMDD}` and a sequence number for `{NUM}`, starting at `1` and incrementing for each additional sync the same day (e.g. `yordis/chore-sync-upstream-20260729-1`). Check `git branch --list --all 'yordis/chore-sync-upstream-*'` and `git worktree list` to pick the next free number.

Run every later command, including verification, from inside this worktree. Setup installs from the pre-merge lockfile; after resolving `pnpm-lock.yaml`, run `vp i` in the worktree again.

## 3. Resolve conflicts

Default to upstream's shape and re-apply the fork's intent on top of it. Concretely:

- If the conflicting area belongs to a ledger divergence, keep the fork behavior and adapt it to upstream's new structure. Do not silently drop a divergence.
- If the fork's version of something is a superset of what upstream added, keep the fork's version and skip the redundant upstream piece (this happened in PR #9 with bootstrap-dispatch logic already covered by the fork's `ThreadBootstrapService`).
- If upstream merged an equivalent of a fork divergence, take upstream's version and delete the corresponding ledger entry outright (numbers are never reused; git history is the record).
- Everything else: take upstream.
- For conflicts under `.repos/` (vendored subtrees), do not hand-merge; take either side to complete the merge, then regenerate with `vpr sync:repos` (or `vpr sync:repos --repo <id>`) so `.repos/` matches the installed dependency versions.

Record every nontrivial resolution decision as you go; each one becomes a PR body bullet.

### Generated files

- `apps/web/src/routeTree.gen.ts` is written by the TanStack router plugin whenever Vite runs, including the test suite in step 4. Accept git's merge result for it, and if the verification run rewrites it, commit the rewritten file. Never resolve it by hand and never restore the fork's copy to quiet a diff: the merge base then carries the stale content and every later sync keeps it, which is how the generator header went missing for a week.

### Migration rules (apps/server/src/persistence)

- Upstream migrations sync as-is, keeping their own ids. `Migrations.ts` and every file under `apps/server/src/persistence/Migrations/` must stay byte-identical to upstream. If either conflicts during a sync, take upstream's side wholesale rather than hand-merging.
- The fork currently carries no schema of its own.
- If fork-only schema is ever needed again, it must live on a separate ledger table, never in `Migrations.ts`, so fork and upstream migration ids can never collide.

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
4. Share the PR link, then remove the worktree from the main checkout (`git worktree remove ../t3code-wt-sync-upstream-{YYYYMMDD}-{NUM}`) and delete the sync branch.
