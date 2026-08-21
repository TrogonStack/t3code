# 0019: The checks badge answers the keyboard

- PR: [TrogonStack/t3code#32](https://github.com/TrogonStack/t3code/pull/32)
- Status: active

## What you can do now

- Tab to the checks badge on a pull request and press Enter or Space to see
  what CI said. The badge has always looked and focused like a button, and it
  now behaves like one, so reading a run's status no longer needs a mouse.
- Keep the console useful while working on pull requests. The mismatch used to
  be reported as an error on every render of a pull request that has checks,
  which is the kind of steady noise that teaches people to stop reading the
  console at all.

## Why

The badge sits inside a listing row that is itself a button, which is the whole
reason it is not one: a button cannot nest inside another. What it had instead
was the appearance of a button without any of the behavior, and that is the
worst of the three options. A plain icon would at least tell a keyboard user
there is nothing here for them. An element that takes focus and announces
itself as a button promises an action, and then swallows the key that would
have taken it.

Checks are also the part of a pull request people look at most often and act on
fastest, so it is a bad place to make someone reach for the mouse.

## Upstream considerations

This belongs upstream and is a good candidate for it: the fix is three lines in
one component, it changes no API, and the behavior it restores is the one the
markup was already claiming. Upstream's own UI library reports the problem, so
the change reads as taking the library's advice rather than as a preference of
ours.

The rebase burden is close to nothing. If upstream ever converts the badge to a
real button, the divergence disappears and this entry goes with it.
