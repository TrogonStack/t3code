# 0034: A pull request watch remembers your instructions

- PR: [TrogonStack/t3code#179](https://github.com/TrogonStack/t3code/pull/179)
- Status: active

## What you can do now

- Give an agent standing instructions for babysitting a pull request, such as ignoring a review
  bot's nits, and have them repeated on every wake.
- Keep those instructions through context compaction and when a watch is stopped and started
  again, until you change or clear them.

## Why

Instructions for a watch lived only in the conversation, so compaction lost them and a wake never
reminded the agent what the user wanted done with the pull request.

## Upstream considerations

Not yet proposed. It touches the watch contract, the watch tool, and the wake text, which upstream
keeps reworking, so it conflicts on most syncs. Proposing it upstream is the way to stop paying
for that.
