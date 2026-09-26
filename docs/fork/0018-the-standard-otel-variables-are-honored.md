# 0018: The standard OTEL variables are honored

- PR: [TrogonStack/t3code#31](https://github.com/TrogonStack/t3code/pull/31)
- Status: active

## What you can do now

- Point T3 Code at your collector the same way you point everything else at
  it. A machine that already exports `OTEL_EXPORTER_OTLP_ENDPOINT` gets T3 Code
  traces, metrics, and log records with no extra configuration, at the
  per-signal paths the specification defines.
- Send the credentials your collector requires. `OTEL_EXPORTER_OTLP_HEADERS`
  reaches the exporter, including the proxy that forwards browser traces, so an
  authenticated endpoint stops rejecting the whole stream.
- Tell your instances apart. `OTEL_SERVICE_VERSION` and
  `OTEL_RESOURCE_ATTRIBUTES` are attached to every span, metric, and log
  record, so T3 Code sits in the same dashboards as everything else. Service
  names themselves are static, and `OTEL_SERVICE_NAME` or a `service.name` in
  `OTEL_RESOURCE_ATTRIBUTES` is refused with a startup warning instead of being
  dropped in silence.
- Configure each signal on its own, logs included. A signal with its own
  address, wire format, or credentials is honored without disturbing the other
  two, `OTEL_LOGS_EXPORTER=none` stops log export while leaving spans and
  metrics alone, and `OTEL_BLRP_SCHEDULE_DELAY` and
  `OTEL_BLRP_MAX_EXPORT_BATCH_SIZE` are the batching knobs the specification
  defines for logs, so a delay meant for spans does not decide how promptly a
  log record arrives.
- Turn export off from the environment. `OTEL_SDK_DISABLED=true` stops every
  export, including one configured in Settings, which is the one switch a
  shared machine needs. `T3CODE_OTEL_SDK_DISABLED` is the same setting asked of
  T3 Code's own name first, so `false` there keeps T3 Code exporting on a
  machine whose profile disables every other SDK.
- Keep whatever you have. The `T3CODE_OTLP_*` names still win, and a setup that
  never mentioned OpenTelemetry keeps the wire format it always used. The
  standard names are read directly under T3 Code's own and above the desktop
  bootstrap envelope and Settings, because an exported variable is what the
  operator asked for now and a stored one is what somebody asked for once.
- Find out when a variable did not take. A misspelled protocol, a temporality
  this exporter cannot produce, a batch size that is not a number, or a header
  list that is not valid percent encoding is named in the startup log and then
  ignored, instead of silently changing nothing or quietly turning export off.
  One bad value costs you that value and nothing else.

## Why

T3 Code has had a real OTLP exporter for a while, and it was unreachable for
almost everyone who wanted it. You had to learn a second set of names for
settings you had already configured once, and the standard names for headers,
resource attributes, and the wire format reached nothing, so an authenticated
collector or a protobuf-only one simply could not be used. T3 Code has its own
header and protocol names now, but the standard ones still arrive through this
divergence.

The cost of that shows up as silence rather than as an error. Someone with a
collector in their shell profile reasonably assumes the app found it, sees a
tidy local trace file, and never learns that nothing left the machine. Reading
the variables everyone else reads turns a feature that existed on paper into one
people can actually reach.

Auto-enabling from an ambient endpoint is the deliberate part. Every other
OpenTelemetry SDK behaves this way, and a telemetry variable that some processes
honor and others quietly ignore is worse than either answer, so
`OTEL_SDK_DISABLED` is the way out rather than a requirement to opt in.

Turning export off is one setting with two names, not two switches, and it is
read in the same order as everything else here: ours, then the standard one.
The ordering is the whole point. Inheriting `OTEL_SDK_DISABLED` from a shell
profile is common, and without a name of our own the only way to get T3 Code's
telemetry back would be to unset a variable the rest of the machine depends
on.

## Upstream considerations

Nothing here is fork-specific and it belongs upstream. Upstream has taken the
kill switch, the standard endpoint, header, and protocol variables, resource
attributes, and static service names. Its reader is a subset of this one, so the
sync keeps this reader and the fork carries the rest of this page: exporter
selection, the batching and temporality knobs, `OTEL_SERVICE_VERSION`, the
refusal warning for a service name, and a bad value costing that value rather
than the whole signal. The riskiest part for
them is the same part that makes it useful: an ambient endpoint starts an export
that includes thread ids, turn ids, and workspace paths, and upstream may prefer
an explicit opt-in for a product with this many users.

The rebase burden is small. The reading lives in one module with no dependencies
on the rest of the server, and the wiring is one call per signal inside existing
precedence chains. A sync that rewrites those chains must keep the standard
names directly under the `T3CODE_OTLP_*` ones and above the desktop bootstrap
envelope and Settings, and must keep a signal those names switched off from
falling through to the stored endpoint underneath it. `resolveSignalSource` is
where that order lives, so both processes move together.
