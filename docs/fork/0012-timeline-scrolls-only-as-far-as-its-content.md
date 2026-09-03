# 0012: The timeline scrolls only as far as its content

- PR: [TrogonStack/t3code#21](https://github.com/TrogonStack/t3code/pull/21)
- Status: active

## What you can do now

- Keep a thread open all day without its scroll range drifting. The blank
  region below the last message no longer accumulates: it used to compound
  with every turn, several times over, until scrolling to the bottom meant
  travelling multiples of the thread's real height through nothing, and
  closing and reopening the thread was the only way out.
- Switch threads in the middle of a jump to the newest message and land in a
  thread that scrolls to its own length, not to the one you left.

## Why

The timeline is the one surface a user never leaves, and its scrollbar is how
they judge how much thread there is. When the scroll range says a thread is
eight times longer than it is, every estimate the user makes from it is wrong,
and the flick that should reach the newest message lands nowhere.

It also degraded the longer someone stayed, which is the worst shape for this
kind of bug: the threads worst affected are exactly the ones people care most
about. That is not something we can ask of people who drive agents all day.

## Upstream considerations

Upstream has since taken the larger half of this divergence: the anchored end
space is now clamped to the content it can actually account for, and the
temporary scroll padding is recorded as the value the browser normalised it
to. Both live in upstream's own copy of the dependency patch, so the sync
takes upstream's version of them and this entry no longer claims them.

What the fork still carries is the part that makes taking that padding back
down reliable: the restore tolerates a browser's own rounding of the value it
was handed, it recognises a second inflation of the same node instead of
losing the original baseline to it, and it runs on unmount so a thread switch
mid-adjust cannot leave inflated padding behind. It also applies to the
React-Native-web entry points, which upstream's version does not touch.

This one is not going upstream, by our choice: it lives in a patch against a
third-party list dependency rather than in T3 Code's own code, so there is no
upstream T3 Code change to submit. The fix belongs to the list library, and we
are carrying it locally instead of taking on that project's release cycle.

The rebase burden is small and self-contained, but it is no longer free: the
patch is regenerated against upstream's copy on each sync rather than merged
line by line, because hand-merging hunk offsets in a generated patch is how
you get a patch that no longer applies. A dependency version bump means
regenerating it too, and whoever bumps it needs to confirm the fix is still
present rather than assume the new version carries it.
