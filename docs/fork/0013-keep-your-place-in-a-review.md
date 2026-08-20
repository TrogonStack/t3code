# 0013: Keep your place in a long review

- PR: [TrogonStack/t3code#23](https://github.com/TrogonStack/t3code/pull/23)
- Status: active

## What you can do now

- Tick a file off as you finish reading it. The file collapses out of the way,
  and the toolbar keeps a running count of how much of the change is behind
  you, so a two hundred file review stops being a wall and becomes a list you
  work down.
- Come back to a review on another machine, or in the browser, and find the
  same files already ticked. The marks are kept with the pull request itself
  rather than in this app, so they are the same marks GitHub shows you.
- Untick a file to open it back up when a second read is warranted.
- See a file you already cleared come back marked as changed once somebody
  pushes to it, so a late commit cannot slip past a review that was finished
  before it landed.
- Read a change one commit at a time and still tick files off, since the mark
  belongs to the pull request rather than to the commit you happen to be
  looking at.

## Why

Reviewing in this app was fine for a small change and unusable for a large
one. Nothing remembered where you were, so a review spread over an afternoon,
or picked up on a different device, started again from the top every time. The
practical result was that big reviews went to the browser and only small ones
stayed here, which undermines the reason to read code in this app at all.

Keeping the marks on the host rather than locally is the part that matters.
A checkbox that only this app remembers is worse than no checkbox: it looks
like the one GitHub shows, disagrees with it, and leaves you unsure which of
the two knows what you have actually read. Deferring to the host means there
is exactly one answer, and switching between this app and the browser
mid-review costs nothing.

## Upstream considerations

Worth submitting. It is a plain product gap rather than anything specific to
how we work, and the shape it takes here is the one upstream would want: the
capability is declared per provider, so GitHub offers it and the hosts that
have no equivalent hide it rather than showing a control that cannot work.

The rebase burden is moderate. It touches the pull request code tab, the
provider port, and the wire contracts, all of which upstream changes often, so
a sync is likely to want a hand in those files. The pieces that carry the
reasoning are in files of their own, which keeps the conflicts to the wiring.
