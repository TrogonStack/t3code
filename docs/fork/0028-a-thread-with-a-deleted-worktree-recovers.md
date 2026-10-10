# 0028: A thread whose worktree was deleted recovers on the next message

- PR: [TrogonStack/t3code#89](https://github.com/TrogonStack/t3code/pull/89)
- Status: active

## What you can do now

- Keep using a thread after its worktree folder and branch were both deleted
  outside T3 Code. The next message recreates the worktree, recreating the
  branch of the same name from the project's current branch when needed, and
  the turn runs instead of failing on every message.
- Read a specific reason when the worktree cannot be recreated, naming the
  branch and folder it tried, instead of a request to restore a folder that no
  longer exists anywhere.

## Why

Cleaning up worktrees and branches from a terminal is routine, and the thread
that owned them was left permanently stuck. Every message asked the user to
restore a folder they had no way to bring back, so the only way out was to
abandon the thread and its history.

## Upstream considerations

A good upstream candidate: it fixes a dead end any upstream user can hit, and
it stays inside the turn start path. The failure message is built from the
branch and folder only, matching upstream's rule that raw git output never
reaches a persisted error. The rebase burden is that one service, which
upstream changes often.
