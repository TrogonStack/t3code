# 0018: The standard OTEL variables are honored

- PR: [TrogonStack/t3code#31](https://github.com/TrogonStack/t3code/pull/31)
- Status: active

## What you can do now

- Point T3 Code at your collector the same way you point everything else at
  it. A machine that already exports `OTEL_EXPORTER_OTLP_ENDPOINT` gets T3 Code
  traces and metrics with no extra configuration, at the per-signal paths the
  specification defines.
- Send the credentials your collector requires. `OTEL_EXPORTER_OTLP_HEADERS`
  reaches the exporter, including the proxy that forwards browser traces, so an
  authenticated endpoint stops rejecting the whole stream.
- Tell your instances apart. `OTEL_SERVICE_VERSION` and
  `OTEL_RESOURCE_ATTRIBUTES` are attached to every span and metric, so T3 Code
  sits in the same dashboards as everything else. Service names themselves are
  static, and `OTEL_SERVICE_NAME` is refused with a warning; see 0023.
- Turn export off from the environment. `OTEL_SDK_DISABLED=true` stops every
  export, including one configured in Settings, which is the one switch a
  shared machine needs.
- Keep whatever you have. The `T3CODE_OTLP_*` names, the desktop bootstrap
  envelope, and Settings all still win over the environment, and a setup that
  never mentioned OpenTelemetry keeps the wire format it always used.
- Find out when a variable did not take. A misspelled protocol, a temporality
  this exporter cannot produce, a batch size that is not a number, or a header
  list that is not valid percent encoding is named in the startup log and then
  ignored, instead of silently changing nothing or quietly turning export off.
  One bad value costs you that value and nothing else.

## Why

T3 Code has had a real OTLP exporter for a while, and it was unreachable for
almost everyone who wanted it. You had to learn a second set of names for
settings you had already configured once, and headers, resource attributes, and
the wire format had no names at all, so an authenticated collector or a
protobuf-only one simply could not be used.

The cost of that shows up as silence rather than as an error. Someone with a
collector in their shell profile reasonably assumes the app found it, sees a
tidy local trace file, and never learns that nothing left the machine. Reading
the variables everyone else reads turns a feature that existed on paper into one
people can actually reach.

Auto-enabling from an ambient endpoint is the deliberate part. Every other
OpenTelemetry SDK behaves this way, and a telemetry variable that some processes
honor and others quietly ignore is worse than either answer, so `OTEL_SDK_DISABLED`
is the way out rather than a requirement to opt in.

## Upstream considerations

Nothing here is fork-specific and it belongs upstream. The riskiest part for
them is the same part that makes it useful: an ambient endpoint starts an export
that includes thread ids, turn ids, and workspace paths, and upstream may prefer
an explicit opt-in for a product with this many users.

The rebase burden is small. The reading lives in one module with no dependencies
on the rest of the server, and the wiring is a handful of fallbacks at the end of
existing precedence chains. A sync that rewrites those chains must keep the
environment as their last entry.
