# 0014: The base branch stays readable in the pull request header

- PR: [TrogonStack/t3code#24](https://github.com/TrogonStack/t3code/pull/24)
- Status: active

## What you can do now

- See where a change is going without hovering for it. The base branch keeps
  enough room for a name you can recognize rather than collapsing to a letter
  and an ellipsis as soon as the panel narrows.
- Open a review beside a thread, at the width most reviews are actually read
  at, and still have both branch names legible. The head name gives up room
  first now, since it is usually the branch you are already on.

## Why

The header's whole job in one line is to say what is being merged into what.
When the base gives way first, the header keeps the branch the reader already
knows and drops the one they opened the review to check, which is backwards.

It was worst exactly where it mattered most. A wide window hid the problem
entirely, so the panel widths people use every day, docked next to a thread,
were the only ones showing a base branch cut down to nothing.

## Upstream considerations

Worth submitting. It is a plain layout bug with nothing fork specific about
it, and upstream has the same header.

The rebase burden is negligible. It is a small change to how one row shares
its width, so a sync carries it untouched unless upstream reworks that header,
in which case reapplying it is a matter of restoring which side absorbs the
slack.
