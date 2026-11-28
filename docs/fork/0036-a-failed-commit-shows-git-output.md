# 0036: A failed commit shows what git said

- PR: [TrogonStack/t3code#87](https://github.com/TrogonStack/t3code/pull/87)
- Status: active

## What you can do now

- Read why a commit was refused. When a pre-commit or commit-msg hook rejects a commit, or there
  is nothing to commit, the error carries git's own output instead of a generic failure.

## Why

Hooks are the most common reason a commit fails, and their output is the only place the reason
lives: a missing sign-off, a lint failure, a formatter that changed files. Hiding it leaves the
reader rerunning the commit in a terminal to find out what the app already knew.

## Upstream considerations

Upstream deliberately keeps hook output out of errors elsewhere, since a local hook can print text
that looks like git's own diagnostics. That makes this a weaker upstream candidate; expect to carry
it. The rebase burden is one branch on the commit command's exit code, which upstream edits now and
then.
