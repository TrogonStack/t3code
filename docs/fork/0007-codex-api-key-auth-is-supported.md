# 0007: API-key Codex installs are not reported as broken

- PR: [TrogonStack/t3code#15](https://github.com/TrogonStack/t3code/pull/15)
- Status: active

## What you can do now

- Run Codex authenticated with an API key instead of a ChatGPT account
  without a warning and a stack trace on every server start. Telemetry falls
  back to the anonymous identifier quietly, the same way it already did when
  there is no Codex auth file at all.
- An auth file that really is unreadable or corrupt still warns, so the
  warning keeps meaning something.

## Why

API keys are a supported way to authenticate Codex, so an install using one
is healthy and should not read as a malfunction in the logs. A warning that
fires on every start of a working setup is worse than no warning, because it
teaches you to scroll past the one that matters.

## Upstream considerations

Nothing here is fork-specific, so this belongs upstream as an ordinary bug
fix. Submit it, then delete this entry once it merges. The surface is small
enough that carrying it in the meantime costs nothing at sync time, though it
does sit in a shared file, so a sync must not drop it.
