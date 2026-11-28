# 0040: A lost provider session only stops its own work

- PR: [TrogonStack/t3code#77](https://github.com/TrogonStack/t3code/pull/77)
- Status: active

## What you can do now

- Keep a thread's other work running when one of its provider sessions dies or is released.
  Only the work that session owned is cancelled and marked interrupted, and the run does not
  wedge waiting on it.

## Why

When a provider session went away, cancelling every unsettled piece of work on the thread
stopped work that belonged to other sessions too, and could leave the run stuck until it was
stopped by hand.

## Upstream considerations

A reasonable candidate to submit as a bug fix. The rebase burden is the cancellation of
unsettled work, which upstream extends now and then, so a sync touching it needs to keep the
narrowing to one provider session.
