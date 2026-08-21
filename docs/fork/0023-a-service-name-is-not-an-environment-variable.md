# 0023: A service name is not an environment variable

- PR: [TrogonStack/t3code#36](https://github.com/TrogonStack/t3code/pull/36)
- Status: active

## What you can do now

- Trust what a service name means. T3 Code reports as `t3-server`, `t3-desktop`,
  and `t3-web`, always, and nothing in the environment can change that. A
  dashboard built on one of those names keeps meaning what it meant when you
  built it.
- Set `OTEL_SERVICE_NAME` on a machine without consequence. It is refused, and
  the refusal is named in the startup log rather than applied quietly or
  dropped quietly.
- Keep telling your instances apart. `OTEL_RESOURCE_ATTRIBUTES` still works and
  is the right lever for it, including `service.instance.id`,
  `deployment.environment`, and `host.name`.
- Rename the server on purpose if you need to. `T3CODE_OTLP_SERVICE_NAME` is
  still honored, because it is T3 Code's own variable and nobody exports it
  fleet-wide by accident.

## Why

This reverses part of 0018, which honored `OTEL_SERVICE_NAME` because every
other OpenTelemetry SDK does. Consistency was the wrong thing to optimize for
here.

A service name is not a preference, it is a key. Endpoints, headers, protocols,
and resource attributes are all things a machine legitimately knows better than
the app does, which is why reading them from the environment is right. A service
name is the opposite: it is the identity every dashboard, alert, and saved query
is keyed on, and it is worth exactly as much as it is stable. One ambient
variable, exported years ago in a shell profile for a different app, silently
merges T3 Code into that app's dashboards and pulls it out of its own.

The failure mode is what makes it worth diverging over. It is invisible from
inside the app, the telemetry keeps flowing, and every panel still renders. What
you get is not an empty dashboard, which someone would investigate, but a
plausible one that is quietly describing two applications at once.

Worth being honest that the specification does not settle this. It fixes the
precedence between `OTEL_SERVICE_NAME` and a `service.name` in
`OTEL_RESOURCE_ATTRIBUTES`, and then leaves environment-versus-code to whichever
order an SDK happens to merge its resources in. Most SDKs let hardcoded values
win, which is the same answer this reaches. The difference is that they reach it
silently and this says so out loud.

## Upstream considerations

The server half of this is not a divergence at all: it restores upstream's
behavior, which was static `t3-server` with `T3CODE_OTLP_SERVICE_NAME` as the
only override. 0018 is what moved away from that, and this moves back. Upstream
taking 0018 should take this with it.

The rebase burden is one line per process plus the absence of a field. The
environment reader has no `serviceName` in its resource type at all, so a sync
that reintroduces the variable has to add the field back before it can be wired
anywhere, rather than silently succeeding.
