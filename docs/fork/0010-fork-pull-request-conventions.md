# 0010: Pull request conventions of our own

- PR: [TrogonStack/t3code#19](https://github.com/TrogonStack/t3code/pull/19)
- Status: active

## What you can do now

- Ask for a PR and get one on this fork. Upstream is only the target when you
  say so, so no branch reaches the upstream project by accident.
- Get a PR body that describes the change and nothing else. Whether to say
  what produced it is yours to decide per PR, not a standing convention.

## Why

Upstream's contributing guidance is written for people contributing to
upstream, where the fork is not a concept and its own repository is the
obvious base. Read literally here, it points every PR at a project that is not
ours. That one is worth writing down rather than leaving to inference, because
it is a one way door: opening a PR against upstream publishes the branch
there, and closing it afterwards does not take it back.

The authorship footer is the ordinary case of a convention that should not be
inherited without asking. Attribution is a call for whoever owns the
repository.

## Upstream considerations

Fork local by definition. Upstream should keep the conventions it has, and
neither change is something to submit. Both edit the shared instructions file,
which is exactly the kind of edit a sync quietly reverts, so a rebase has to
carry them forward deliberately.
