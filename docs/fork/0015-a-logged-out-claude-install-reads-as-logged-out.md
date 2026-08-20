# 0015: A logged-out Claude install reads as logged out

- PR: [TrogonStack/t3code#26](https://github.com/TrogonStack/t3code/pull/26)
- Status: active

## What you can do now

- Tell at a glance whether a Claude instance can actually run. One whose CLI
  holds no credentials reads as not authenticated in Settings and in the chat
  banner, with the prompt to sign in that every other signed-out provider
  already gets, instead of claiming to be authenticated and failing on the
  first message you send it.
- Keep trusting the badge on instances that authenticate in the less common
  ways. API-key installs, Bedrock and Vertex backends, and gateway or profile
  setups still report as authenticated, none of which carry an account the way
  a signed-in first-party install does.

## Why

Settings answers one question: is this provider working. Claude answered it by
printing "Authenticated" for any instance whose CLI started at all, which is a
different question from whether that CLI has anything to authenticate with.

The gap only opens where several Claude accounts run side by side, each bound
to its own credential store, and that is exactly where the answer needs to be
right: instances that had never held a credential showed the same green badge
as the working one, and the only way to find out which was which was to start
a thread and watch it fail. A status that is correct in the ordinary case and
wrong in precisely the case you consulted it for is worse than no status,
because it spends the trust that makes the rest of the page worth reading.

## Upstream considerations

Nothing here is fork-specific, so this belongs upstream as an ordinary bug
fix. Submit it, then delete this entry once it merges. It sits in the shared
Claude provider status check, so a sync must not drop it. The rebase burden is
small: the decision is one exported function over the capability probe's own
fields, and the only other change is a field the probe already had from the
SDK and was discarding.
