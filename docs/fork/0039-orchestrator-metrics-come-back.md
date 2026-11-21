# 0039: Turn, session, and command metrics are recorded again

- PR: [TrogonStack/t3code#76](https://github.com/TrogonStack/t3code/pull/76)
- Status: active

## What you can do now

- Chart provider turns, provider sessions, and orchestration commands from the metrics a
  T3 Code server exports, including how long each command took to commit.

## Why

Replacing the old orchestration stack removed every provider and orchestration metric along
with it. Dashboards built on them went empty with nothing to say they had regressed, and the
traces alone do not answer how many turns ran or how long commands wait.

## Upstream considerations

A good candidate to propose, since it restores numbers upstream used to export. The rebase
burden sits in the orchestrator's dispatch path: every new place upstream dispatches a command
needs the dispatch start time passed along, so a sync that adds dispatch sites has to carry it.
