# 0045: Watch Dog babysits a pull request until it is green

- PR: [TrogonStack/t3code#177](https://github.com/TrogonStack/t3code/pull/177)
- Status: active

## What you can do now

- Hand an open pull request to an agent with one button in its header. The agent fixes what is
  already broken, then watches the pull request and keeps fixing failed checks, review remarks,
  and merge conflicts until it is green.
- See at a glance whether a pull request needs that. The button reads "All clear" when checks
  pass with nothing left to answer, turns red when jobs fail, threads are unresolved, a review
  requests changes, or the branch conflicts, and its tooltip says which.
- Trust "All clear". It is never shown while checks, review threads, or comments are still
  loading or were cut short.

## Why

Babysitting a pull request through CI and review is the most repeated hand-off in this fork's
workflow. Upstream offers the watch itself but no one-click way to start it from the pull
request, and no summary of whether the pull request still needs attention.

## Upstream considerations

A candidate once the prompt and status rules settle. The rebase burden sits in the pull request
header and detail panel, which upstream edits often (this sync moved its tooltip to the shared
tooltip), so a sync touching either has to keep the button and its status.
