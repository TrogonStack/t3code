# 0012: The timeline scrolls only as far as its content

- PR: [TrogonStack/t3code#21](https://github.com/TrogonStack/t3code/pull/21)
- Status: active

## What you can do now

- Drag the timeline scrollbar and land where the thumb says you will. A long
  thread no longer grows an empty region below the last message that the
  scrollbar still counts, so the thumb stays proportional to the conversation
  instead of shrinking to a sliver.
- Keep a thread open all day without its scroll range drifting. The dead space
  used to compound with every turn, several times over, until scrolling to the
  bottom meant travelling multiples of the thread's real height through
  nothing.
- Flick to the end on a phone and stop at the last message rather than
  somewhere in the blank area past it.

## Why

The timeline is the one surface a user never leaves, and its scrollbar is how
they judge how much thread there is. When the scroll range says a thread is
eight times longer than it is, every estimate the user makes from it is wrong,
and the flick that should reach the newest message lands nowhere.

It also degraded the longer someone stayed, which is the worst shape for this
kind of bug: the threads worst affected are exactly the ones people care most
about, and closing and reopening the thread was the only way out. That is not
something we can ask of people who drive agents all day.

## Upstream considerations

This one is not going upstream, by our choice: it lives in a patch against a
third-party list dependency rather than in T3 Code's own code, so there is no
upstream T3 Code change to submit. The fix belongs to the list library, and we
are carrying it locally instead of taking on that project's release cycle.

The rebase burden is small and self-contained. It survives a sync untouched
unless the dependency version moves; a version bump means regenerating the
patch, and the affected code was unchanged in the next upstream release, so it
should reapply. Anyone bumping that dependency needs to confirm the fix is
still present rather than assume the new version carries it.
