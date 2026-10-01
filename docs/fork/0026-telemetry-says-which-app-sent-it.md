# 0026: Telemetry says which app sent it

- PR: [TrogonStack/t3code#68](https://github.com/TrogonStack/t3code/pull/68)
- Status: active

## What you can do now

- Filter UI traces to the desktop window or to a browser tab with
  `t3code.client.surface`, instead of guessing from a key named `service.mode`.
- Tell a server the desktop app launched from one you started yourself with
  `t3code.server.managed_by`.
- Query every T3 Code signal with the resource attributes a collector,
  dashboard, or vendor already understands: `service.version`,
  `deployment.environment.name`, `process.runtime.*`, `user_agent.original`,
  and `browser.*`.
- Set `deployment.environment.name` through `OTEL_RESOURCE_ATTRIBUTES` and
  have the desktop app keep it.

## Why

The desktop window runs the web UI, so its traces arrive as `t3code-web`, and
the only thing separating them from a browser tab was `service.mode`. That
key sat in the `service.*` namespace, which OpenTelemetry reserves for its
own attributes, and it meant something different in each service: where the
UI runs in one, the build type in another, and who launched the process in
the third. Nobody reading a trace could know that without reading the code.

Following the semantic conventions puts every attribute where tooling
already looks for it, and keeps the one question the conventions have no
answer for, which surface of the product sent this, under the `t3code.`
prefix. That is the application's own name, which the conventions recommend
for custom attributes, and unlike `t3` it cannot be mistaken for anything
else.

## Upstream considerations

A plausible upstream submission. The attributes came from upstream, and the
change carries no fork-specific intent. Dashboards or saved queries that
filter on `service.mode`, `service.runtime`, `service.component`, or the
relay's `t3.client.surface` have to move to the new keys, which is the part upstream would want to weigh.

The server's own `mode` values (`web` for any standalone launch) are left
alone, since renaming them touches the CLI flag, `T3CODE_MODE`, and persisted
session data. Telemetry maps `mode` to `t3code.server.managed_by` instead. A
sync that takes upstream's copy of any resource definition brings the old
keys back without any test going red outside the ones changed here.
