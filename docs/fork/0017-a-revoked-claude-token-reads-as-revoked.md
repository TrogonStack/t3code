# 0017: A revoked Claude token reads as revoked

- PR: [TrogonStack/t3code#28](https://github.com/TrogonStack/t3code/pull/28)
- Status: active

## What you can do now

- Trust the badge on a Claude instance that authenticates with a setup token.
  If Anthropic has stopped accepting that token, Settings says so and tells you
  to mint a new one, instead of reporting the instance as authenticated until
  the first message you send it fails.
- Keep the status you had when the value is not a credential at all. A
  placeholder, or a reference to a secret store that nothing resolved, leaves
  the instance as it was instead of blaming a token that was never there.
- Keep the status you had when the network is the problem. An outage, a proxy,
  or a captive portal leaves the instance exactly as it was, so a bad connection
  never looks like a revoked credential.
- Keep trusting every other kind of Claude install. Instances signed in with
  `claude auth login`, running on an API key, pointed at a router, or backed by
  Bedrock or Vertex are reported exactly as before.

## Why

Settings answers one question: is this provider working. Entry 0015 taught it to
notice an install that holds no credential at all, but a credential that exists
and no longer works looked identical to one that does, and that is the more
common failure. Setup tokens expire on their own schedule and get revoked out
from under you.

This matters most where several Claude accounts run side by side, each holding
its own token. That is exactly the setup where you consult the badge rather than
already knowing the answer, and a badge that is right about the accounts you were
not worried about while staying green on the one that just broke spends the trust
that makes the rest of the page worth reading.

## Upstream considerations

Nothing here is fork-specific and it belongs upstream, but it is a weaker
candidate than 0015 because it puts a direct Anthropic request in the provider
status path and relies on an authentication mode Anthropic has not published.
Both are reasons upstream might decline, so expect to carry it.

The rebase burden is small and well contained: the request lives in its own
module, and the status check gains one branch after the decision entry 0015
already introduced. A sync must not drop that branch.
