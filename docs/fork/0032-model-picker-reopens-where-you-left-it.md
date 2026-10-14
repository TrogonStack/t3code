# 0032: The model picker reopens where you last picked from

- PR: [TrogonStack/t3code#103](https://github.com/TrogonStack/t3code/pull/103)
- Status: active

## What you can do now

- Switch models within the account you are already using without finding that
  account again every time. When your last pick came from an account, the
  picker opens on the account you are using now.
- Keep driving the picker from Favorites. When your last pick came from
  Favorites, it opens there, so the numbered shortcuts still land on the same
  favorites.
- Open the picker before your favorites have finished loading and still land
  on Favorites once they arrive, unless you already picked a tab yourself.

## Why

Upstream always opened the picker on Favorites. That made sense when each tab
in the rail was a provider, but once each tab became an account, someone with
several accounts per provider spent every model switch hunting for the account
they were already in.

Always opening on the active account instead would trade one habit for
another. The numbered shortcuts follow whichever list is showing, so people
who switch through Favorites by keyboard would find their shortcuts pointing
at different models. Remembering where the last pick came from keeps both
habits working.

## Upstream considerations

A reasonable candidate to submit, since it changes a default rather than
adding anything fork specific. Upstream chose the Favorites default on
purpose, so the proposal has to make the multi-account case for changing it.

The rebase burden is small but sits in a component upstream changes often, so
a sync that touches how the picker chooses its opening tab needs to carry the
remembered choice forward deliberately.
