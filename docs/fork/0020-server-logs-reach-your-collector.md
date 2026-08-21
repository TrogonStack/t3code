# 0020: Server logs reach your collector

- PR: [TrogonStack/t3code#33](https://github.com/TrogonStack/t3code/pull/33)
- Status: active

## What you can do now

- Read T3 Code's server logs where you already read everything else. Log
  records leave for your collector as OTLP, so a session that misbehaved can be
  read next to the spans and metrics it produced instead of only in a file on
  the machine that produced it.
- Configure the log signal exactly the way you configure the other two. The
  standard `OTEL_EXPORTER_OTLP_LOGS_*` variables, the `T3CODE_OTLP_LOGS_URL`
  name, the desktop bootstrap envelope, and Settings all reach it, in the same
  order of precedence traces and metrics already use.
- Send one signal somewhere the others do not go, or turn one off on its own.
  A log endpoint with its own address, wire format, or credentials is honored
  without disturbing spans, and `OTEL_LOGS_EXPORTER=none` stops log export
  while leaving the rest of your telemetry alone.
- Batch log records on their own schedule. `OTEL_BLRP_SCHEDULE_DELAY` and
  `OTEL_BLRP_MAX_EXPORT_BATCH_SIZE` are the knobs the specification defines for
  logs, so a delay meant for spans no longer decides how promptly a log record
  arrives.
- See which signals are actually exporting. Settings names the log endpoint
  next to the trace and metric ones, so a collector that is receiving two of
  the three is visible rather than something you discover from an empty
  dashboard.
- Keep the local log file. Nothing about the export changes what is written to
  disk, so a machine with no collector behaves as it always has.

## Why

Traces and metrics could reach a collector and logs could not, which made the
one signal people reach for first the one signal T3 Code kept to itself. A
maintainer debugging a remote environment had spans showing that a turn took
too long and no way to read what the server said while it happened, short of
asking someone to find a file and paste it.

Partial coverage is also the confusing kind of gap. Someone who exports
`OTEL_EXPORTER_OTLP_ENDPOINT` gets two signals and no indication that the third
was dropped, and a variable the rest of their fleet honors reasonably looks
honored here too. OpenTelemetry defines three signals; supporting two of them is
a bug in the same way that honoring a variable in some processes and not others
is a bug.

Full parity is deliberate rather than incidental. A log signal that only read
the environment, or that ignored its own batching variables and borrowed the
span schedule, would be a second thing to learn instead of one less thing to
work around.

## Upstream considerations

This belongs upstream and is not fork-specific. It completes work upstream
already started, so the argument for it is the same argument that carried
traces and metrics.

The rebase burden is concentrated in one place worth knowing about. Every
logger the server installs has to be declared together, because installing a
logger replaces the whole set rather than adding to it, so the OTLP log exporter
sits beside the console and tracer loggers rather than in the observability
layer with its sibling signals. A sync that adds another logger must add it to
that same set; a second layer that installs one independently silently drops
whichever set loses the merge.
