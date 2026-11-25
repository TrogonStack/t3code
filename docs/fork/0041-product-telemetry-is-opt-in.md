# 0041: Product telemetry is opt-in

- PR: [TrogonStack/t3code#252](https://github.com/TrogonStack/t3code/pull/252)
- Status: active

## What you can do now

- Run the server without sending product usage events or relay connection traces anywhere until
  you set `T3CODE_TELEMETRY_ENABLED=true`.
- Send product usage events only to a PostHog-compatible endpoint you choose. Opting in without
  `T3CODE_POSTHOG_HOST` and `T3CODE_POSTHOG_KEY` still sends nothing.
- Keep your installation free of a telemetry identifier until you opt in.

## Why

Upstream reports to its own endpoints by default. This fork has no reason to send usage data to
upstream, or anywhere, without the user's consent, and opting in should not quietly point at
upstream's ingest key either.

## Upstream considerations

Not a candidate: upstream wants its default-on telemetry. The rebase burden sits in the analytics
service's config and enablement check and in the user telemetry doc, which upstream edits often,
so a sync touching either has to keep the off-by-default setting and the missing default
destination.
