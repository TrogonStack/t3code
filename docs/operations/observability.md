# Observability

> For maintainers. Using T3 Code? See [docs/user](../user/).

T3 Code has one server-side observability model:

- pretty logs go to stdout for humans
- completed spans go to a local NDJSON trace file
- traces and metrics can also be exported over OTLP to a real backend like Grafana LGTM

The local trace file is the persisted source of truth for normal local launches. Those launches do not
write a separate server log file, but SSH-managed launches also persist the remote process's
stdout/stderr at `~/.t3/ssh-launch/<state>/server.log`.

## Where To Find Things

### Logs

Logs are human-facing:

- destination: stdout
- format: `Logger.consolePretty()`
- normal local persistence: none
- SSH-managed launch persistence: `~/.t3/ssh-launch/<state>/server.log`

If you want a log message to show up in the trace file, emit it inside an active span with `Effect.log...`. `Logger.tracerLogger` will attach it as a span event.

### Traces

Completed spans are written as NDJSON records to `serverTracePath`. The default depends on how the
server starts: production and explicitly configured homes use
`<home>/userdata/logs/server.trace.ndjson` (so `~/.t3/userdata/...` by default, or
`/custom/path/userdata/...` with `--home-dir /custom/path`), a linked worktree dev run uses
`<worktree>/.t3/userdata/logs/server.trace.ndjson`, and an implicit dev run outside a linked
worktree uses `~/.t3/dev/logs/server.trace.ndjson`.

Important fields common to both record types:

- `type`: `effect-span` or `otlp-span`
- `name`: span name
- `traceId`, `spanId`, `parentSpanId`: correlation
- `durationMs`: elapsed time
- `attributes`: structured context
- `events`: embedded logs and custom events

`effect-span` records also contain `exit` with `Success`, `Failure`, or `Interrupted`. `otlp-span`
records instead carry OTLP resource, scope, and optional status fields.

The `TraceRecord`, `EffectTraceRecord`, and `OtlpTraceRecord` schemas live in
`packages/shared/src/observability.ts`.

### Metrics

Metrics are not written to a local file.

- local persistence: none
- remote export: OTLP only, when configured
- current definitions: `apps/server/src/observability/Metrics.ts`

If OTLP is not configured, metrics still exist in-process, but you will not have a local artifact to inspect.

### Related Artifacts

Provider event NDJSON files still exist for provider runtime streams. Those are separate from the main server trace file.

## Run The Server In Instrumented Mode

There are two useful modes:

- local-only: stdout + local `server.trace.ndjson`
- full local observability: stdout + local trace file + OTLP export to Grafana/Tempo/Prometheus

The local trace file is always on. OTLP export is opt-in.

### Option 1: Local Traces Only

You do not need any extra env vars. Just run the app normally and inspect `server.trace.ndjson`.

Examples:

```bash
npx t3
```

```bash
node --run dev
```

```bash
node --run dev:desktop
```

### Option 2: Run With A Local LGTM Stack

#### 1. Start Grafana LGTM

```bash
docker run --name lgtm \
  -p 3000:3000 \
  -p 4317:4317 \
  -p 4318:4318 \
  --rm -ti \
  grafana/otel-lgtm
```

Then open `http://localhost:3000`.

Default Grafana login:

- username: `admin`
- password: `admin`

#### 2. Export OTLP env vars

```bash
export T3CODE_OTLP_TRACES_URL=http://localhost:4318/v1/traces
export T3CODE_OTLP_METRICS_URL=http://localhost:4318/v1/metrics
export T3CODE_OTLP_SERVICE_NAME=t3-local
```

Optional:

```bash
export T3CODE_TRACE_MIN_LEVEL=Info
export T3CODE_TRACE_TIMING_ENABLED=true
```

#### 3. Launch the app from that same shell

CLI:

```bash
npx t3
```

Monorepo web/server dev:

```bash
node --run dev
```

Monorepo desktop dev:

```bash
node --run dev:desktop
```

Packaged desktop app:

Launch the actual app executable from the same shell so the desktop app and embedded backend inherit `T3CODE_OTLP_*`.

macOS app bundle example:

```bash
T3CODE_OTLP_TRACES_URL=http://localhost:4318/v1/traces \
T3CODE_OTLP_METRICS_URL=http://localhost:4318/v1/metrics \
T3CODE_OTLP_SERVICE_NAME=t3-desktop \
"/Applications/T3 Code.app/Contents/MacOS/T3 Code"
```

Direct binary example:

```bash
T3CODE_OTLP_TRACES_URL=http://localhost:4318/v1/traces \
T3CODE_OTLP_METRICS_URL=http://localhost:4318/v1/metrics \
T3CODE_OTLP_SERVICE_NAME=t3-desktop \
./path/to/your/desktop-app-binary
```

Do not rely on launching from Finder, Spotlight, the dock, or the Start menu after setting shell env vars. Those launches usually will not pick them up.

#### 4. Fully restart after changing env

The backend reads observability config at process start. If you change OTLP env vars, stop the app completely and start it again.

### Option 3: The Standard `OTEL_*` Variables

If your machine already exports the OpenTelemetry environment variables for everything else running on
it, T3 Code joins in without being told twice. Nothing above is required:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
export OTEL_SERVICE_NAME=t3-local
```

The base endpoint is a base, not a full URL: traces go to `<endpoint>/v1/traces` and metrics to
`<endpoint>/v1/metrics`, exactly as the specification says. Set
`OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` or `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` when a signal needs a
full URL of its own.

Ambient `OTEL_*` variables turn export on by themselves. A work collector in your shell profile means
T3 Code exports to it, so use `OTEL_SDK_DISABLED=true` if that is not what you want.

#### Precedence

For each signal, the first source that names its endpoint wins:

1. `T3CODE_OTLP_*`
2. the desktop bootstrap envelope
3. Settings, under `observability`
4. `OTEL_*`

Whichever source wins takes the whole signal, not just the URL. Traces sent to a
`T3CODE_OTLP_TRACES_URL` endpoint keep T3 Code's own wire format, headers, batching, and export
interval even when `OTEL_*` variables are set, because those variables describe the collector they
named rather than this one. `T3CODE_OTLP_EXPORT_INTERVAL_MS` is the exception, and applies to both
signals wherever they go.

The two signals are resolved separately, so traces can come from one source and metrics from
another.

`OTEL_SDK_DISABLED=true` outranks all four and stops every export, including one configured through
Settings.

#### What Is Read

| Variable                                                                                                  | Effect                                                                       |
| --------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `OTEL_SDK_DISABLED`                                                                                       | Stops all export                                                             |
| `OTEL_EXPORTER_OTLP_ENDPOINT`                                                                             | Base URL for both signals                                                    |
| `OTEL_EXPORTER_OTLP_{TRACES,METRICS}_ENDPOINT`                                                            | Full URL for one signal                                                      |
| `OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_EXPORTER_OTLP_{TRACES,METRICS}_HEADERS`                               | Export headers, per signal overriding the shared ones                        |
| `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_EXPORTER_OTLP_TRACES_PROTOCOL`                                       | `http/protobuf` (default) or `http/json`                                     |
| `OTEL_{TRACES,METRICS}_EXPORTER`                                                                          | A list; the signal is exported when it contains `otlp`, which is the default |
| `OTEL_SERVICE_NAME`, `OTEL_SERVICE_VERSION`, `OTEL_RESOURCE_ATTRIBUTES`                                   | Resource identity attached to every span and metric                          |
| `OTEL_EXPORTER_OTLP_TIMEOUT`, `OTEL_EXPORTER_OTLP_{TRACES,METRICS}_TIMEOUT`, `OTEL_METRIC_EXPORT_TIMEOUT` | Shutdown flush timeout                                                       |
| `OTEL_BSP_SCHEDULE_DELAY`, `OTEL_METRIC_EXPORT_INTERVAL`                                                  | Export interval                                                              |
| `OTEL_BSP_MAX_EXPORT_BATCH_SIZE`                                                                          | Spans per batch                                                              |
| `OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE`                                                       | `cumulative` or `delta`                                                      |

The wire format defaults to `http/protobuf` when the endpoint came from `OTEL_*`, matching the
specification, and stays `http/json` for a `T3CODE_OTLP_*` setup that never mentioned a protocol.

`OTEL_EXPORTER_OTLP_PROTOCOL=grpc` is refused rather than downgraded, because this server has no gRPC
transport and posting an HTTP body to a gRPC endpoint fails in a way that is harder to read than
exporting nothing. The refusal is logged at startup and turns off only the signal that named gRPC,
and only when that signal had no other endpoint to go to.

Header and resource-attribute values are percent decoded, so
`OTEL_EXPORTER_OTLP_HEADERS=Authorization=Bearer%20abc` sends the space and a base64 credential keeps
its `=` padding.

#### Known Gaps

Not everything in the specification is implemented. These are the ones worth knowing about:

- **No gRPC.** `OTEL_EXPORTER_OTLP_PROTOCOL=grpc` is refused rather than downgraded, because this
  server has no gRPC transport and posting an HTTP body to a gRPC endpoint fails in a way that is
  harder to read than exporting nothing. The refusal is logged at startup and turns off only the
  signal that named gRPC, so `OTEL_EXPORTER_OTLP_METRICS_PROTOCOL=grpc` leaves traces exporting.
- **No compression and no client TLS.** `OTEL_EXPORTER_OTLP_COMPRESSION`,
  `OTEL_EXPORTER_OTLP_CERTIFICATE`, `OTEL_EXPORTER_OTLP_CLIENT_KEY`, and
  `OTEL_EXPORTER_OTLP_CLIENT_CERTIFICATE` are ignored. A collector that requires mutual TLS needs a
  proxy in front of it.
- **Timeouts flush at shutdown.** The specification's `OTEL_EXPORTER_OTLP_TIMEOUT` is a per-request
  deadline. The exporter here has no per-request knob, so the value bounds the final flush instead.
- **Browser traces are always JSON.** The proxy that forwards traces from the client posts
  OTLP/HTTP JSON regardless of `OTEL_EXPORTER_OTLP_PROTOCOL`. Both are valid OTLP/HTTP and most
  collectors accept either, so this only matters against one that takes protobuf and nothing else.
  When the trace exporter resolves to `http/protobuf`, a startup warning names the split rather
  than letting the browser half disappear while the server half looks healthy.
- **No protocol name of T3 Code's own.** `OTEL_EXPORTER_OTLP_PROTOCOL` describes the endpoint these
  variables named. A `T3CODE_OTLP_*` endpoint always uses `http/json`, which is what it has always
  used.
- **`lowmemory` temporality is not available.** `OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE`
  accepts `cumulative` and `delta`. `lowmemory` logs a warning and falls back to `cumulative`.
- **`OTEL_SERVICE_VERSION` is not a specification variable.** It is read as a convenience because
  the exporter library reads it too. `OTEL_RESOURCE_ATTRIBUTES=service.version=...` is the portable
  spelling.

Everything else not listed above is ignored, including the log signal, `OTEL_BSP_MAX_QUEUE_SIZE`,
`OTEL_BSP_EXPORT_TIMEOUT`, sampler variables, propagator variables, and the attribute and span
limit variables.

#### When A Value Cannot Be Used

A variable this server cannot act on never stops it from starting. Two things can happen instead,
and both are logged once at startup:

- **A warning, then the default.** A misspelled protocol, an unavailable temporality, or a pair list
  that is not valid percent encoding is reported and ignored, and everything else keeps exporting.
- **Export off.** Only `OTEL_EXPORTER_OTLP_PROTOCOL=grpc` does this, because it names a transport
  this server does not speak rather than a value it failed to parse.

A `OTEL_EXPORTER_OTLP_HEADERS` or `OTEL_RESOURCE_ATTRIBUTES` value that fails to decode is discarded
whole rather than partly. A half-parsed credential reaches the collector as the same authentication
error a wrong one would, which reads like a bad token instead of a bad variable.

These variables configure a signal only when they also supplied its endpoint. A `T3CODE_OTLP_*`
name, the desktop bootstrap envelope, or Settings winning the URL takes the whole signal with it, so
an ambient `OTEL_EXPORTER_OTLP_ENDPOINT` cannot reach in and change the wire format, headers, or
batching of an export it did not point anywhere. Traces and metrics are answered separately
throughout, so `OTEL_EXPORTER_OTLP_METRICS_PROTOCOL` applies to metrics alone and leaves traces as
they were.

Once these variables are the ones configuring the exporter, the specification's own defaults apply:
`OTEL_BSP_SCHEDULE_DELAY` 5s, `OTEL_METRIC_EXPORT_INTERVAL` 60s, and `OTEL_BSP_MAX_EXPORT_BATCH_SIZE` 512. A `T3CODE_OTLP_*` setup keeps the numbers T3 Code has always used.

## How To Use Traces And Metrics To Debug The Server

### Start With The Local Trace File

The trace file is the fastest way to inspect raw span data.

Resolve the path for the launch mode once. Production and explicitly configured homes store runtime
state under the base directory's `userdata` folder:

```bash
TRACE_FILE="${T3CODE_HOME:-$HOME/.t3}/userdata/logs/server.trace.ndjson"
```

A dev server started from a linked worktree defaults to that worktree's local home:

```bash
TRACE_FILE="$WORKTREE/.t3/userdata/logs/server.trace.ndjson"
```

Only an implicit dev run outside a linked worktree uses the shared dev directory:

```bash
TRACE_FILE="$HOME/.t3/dev/logs/server.trace.ndjson"
```

Tail the selected file:

```bash
tail -f "$TRACE_FILE"
```

Show failed spans:

```bash
jq -c 'select(.type == "effect-span" and .exit._tag != "Success") | {
  name,
  durationMs,
  exit,
  attributes
}' "$TRACE_FILE"
```

Show slow spans:

```bash
jq -c 'select(.durationMs > 1000) | {
  name,
  durationMs,
  traceId,
  spanId
}' "$TRACE_FILE"
```

Inspect embedded log events:

```bash
jq -c 'select(any(.events[]?; .attributes["effect.logLevel"] != null)) | {
  name,
  durationMs,
  events: [
    .events[]
    | select(.attributes["effect.logLevel"] != null)
    | {
        message: .name,
        level: .attributes["effect.logLevel"]
      }
  ]
}' "$TRACE_FILE"
```

Follow one trace:

```bash
jq -r 'select(.traceId == "TRACE_ID_HERE") | [
  .name,
  .spanId,
  (.parentSpanId // "-"),
  .durationMs
] | @tsv' "$TRACE_FILE"
```

Filter orchestration commands:

```bash
jq -c 'select(.attributes["orchestration.command_type"] != null) | {
  name,
  durationMs,
  commandType: .attributes["orchestration.command_type"],
  aggregateKind: .attributes["orchestration.aggregate_kind"]
}' "$TRACE_FILE"
```

Filter git activity:

```bash
jq -c 'select(.attributes["git.operation"] != null) | {
  name,
  durationMs,
  operation: .attributes["git.operation"],
  cwd: .attributes["git.cwd"],
  hookEvents: [
    .events[]
    | select(.name == "git.hook.started" or .name == "git.hook.finished")
  ]
}' "$TRACE_FILE"
```

### Use Tempo When You Need A Real Trace Viewer

Tempo is better than raw NDJSON when you want to:

- search across many traces
- inspect parent/child relationships visually
- compare many slow traces
- drill into one failing request without hand-joining by `traceId`

Recommended flow in Grafana:

1. Open `Explore`.
2. Pick the `Tempo` data source.
3. Set the time range to something recent like `Last 15 minutes`.
4. Start broad. Do not begin with a very narrow query.
5. Look for spans from your configured service name, then narrow by span name or attributes.

Good first searches:

- service name such as `t3-local`, `t3-dev`, or `t3-desktop`
- span names like `sendTurn` or a Git operation such as `GitVcsDriver.statusDetails.status`
- Git spans whose `git.operation` attribute identifies the operation
- orchestration spans with attributes like `orchestration.command_type`

Once you know traces are arriving, narrower TraceQL queries for names such as `sendTurn` or Git
operation names become useful.

### Use Metrics To See Systemic Problems

Traces are best for one request. Metrics are best for trends.

Good metric families to watch:

- `t3_rpc_request_duration`
- `t3_orchestration_command_duration`
- `t3_orchestration_command_ack_duration`
- `t3_provider_turn_duration`
- `t3_git_command_duration`

Counters tell you volume and failure rate:

- `t3_rpc_requests_total`
- `t3_orchestration_commands_total`
- `t3_provider_turns_total`
- `t3_git_commands_total`

Use metrics when the question is:

- "is this always slow?"
- "did this get worse after a change?"
- "which command type is failing most often?"

Use traces when the question is:

- "what happened in this specific request?"
- "which child span caused this one slow interaction?"
- "what logs were emitted inside the failing flow?"

### What The New Ack Metric Means

`t3_orchestration_command_ack_duration` measures:

- start: command dispatch enters the orchestration engine
- end: the first committed domain event for that command is published by the server

That is a server-side acknowledgment metric. It does not measure:

- websocket transit to the browser
- client receipt
- React render time

If you need those later, add client-side instrumentation or a dedicated server fanout metric.

## Common Workflows

### "Why did this request fail?"

1. Start with the local NDJSON file.
2. Find `effect-span` records where `exit._tag != "Success"`.
3. Group by `traceId`.
4. Inspect sibling spans and span events.
5. If needed, move to Tempo for the full trace tree.

### "Why is the UI feeling slow?"

1. Search for slow top-level spans in the trace file or Tempo.
2. Check child spans for sqlite, git, provider, or terminal work.
3. Look at the matching duration metrics to see whether the slowness is systemic.

### "Did this command take too long to acknowledge?"

1. Check `t3_orchestration_command_ack_duration` by `commandType`.
2. If it is high, inspect the corresponding orchestration trace.
3. Look at child spans for projection, sqlite, provider, or git work.

### "Are git hooks causing latency?"

1. Filter `git.operation` spans.
2. Inspect `git.hook.started` and `git.hook.finished` events.
3. Compare hook timing to the enclosing git span duration.

### "Why do I have spans locally but nothing in Grafana?"

Usually one of these is true:

- `T3CODE_OTLP_TRACES_URL` was not set
- the app was launched from a different environment than the one where you exported the vars
- the app was not fully restarted after changing env
- Grafana is looking at the wrong time range or service name

If the local NDJSON file is updating, local tracing is working. The problem is almost always OTLP export configuration or process startup.

## How To Think About Adding Tracing To Future Code

### Prefer Boundaries Over Tiny Helpers

Good span boundaries:

- RPC methods
- orchestration command handling
- provider adapter calls
- external process calls
- persistence writes
- queue handoffs

Avoid tracing every tiny helper. Most helpers should inherit the active span rather than create a new one.

### Reuse `Effect.fn(...)` Where It Already Exists

The codebase already uses `Effect.fn("name")` heavily. That should usually be your first tracing boundary.

For ad hoc work:

```ts
import { Effect } from "effect";

const runThing = Effect.gen(function* () {
  yield* Effect.annotateCurrentSpan({
    "thing.id": "abc123",
    "thing.kind": "example",
  });

  yield* Effect.logInfo("starting thing");
  return yield* doWork();
}).pipe(Effect.withSpan("thing.run"));
```

### Put High-Cardinality Detail On Spans

Use span annotations for IDs, paths, and other detailed context:

```ts
yield *
  Effect.annotateCurrentSpan({
    "provider.thread_id": input.threadId,
    "provider.request_id": input.requestId,
    "git.cwd": input.cwd,
  });
```

### Keep Metric Labels Low Cardinality

Good metric labels:

- operation kind
- method name
- provider kind
- aggregate kind
- outcome

Bad metric labels:

- raw thread IDs
- command IDs
- file paths
- cwd
- full prompts
- full model strings when a normalized family label would do

Detailed context belongs on spans, not metrics.

### Use Logs As Span Events

Logs inside a span become part of the trace story:

```ts
yield * Effect.logInfo("starting provider turn");
yield * Effect.logDebug("waiting for approval response");
```

Those messages show up as span events because `Logger.tracerLogger` is installed.

### Use The Pipeable Metrics API

`withMetrics(...)` is the default way to attach a counter and timer to an effect:

```ts
import { someCounter, someDuration, withMetrics } from "../observability/Metrics.ts";

const program = doWork().pipe(
  withMetrics({
    counter: someCounter,
    timer: someDuration,
    attributes: {
      operation: "work",
    },
  }),
);
```

## Detailed API Reference

### Runtime Wiring

The server observability layer is assembled in `apps/server/src/observability/Layers/Observability.ts`.

It provides:

- pretty stdout logger
- `Logger.tracerLogger`
- local NDJSON tracer
- optional OTLP trace exporter
- optional OTLP metrics exporter
- Effect trace-level and timing refs

### Env Vars

Local trace file:

- `T3CODE_TRACE_FILE`: override trace file path
- `T3CODE_TRACE_MAX_BYTES`: per-file rotation size, default `10485760`
- `T3CODE_TRACE_MAX_FILES`: rotated file count, default `10`
- `T3CODE_TRACE_BATCH_WINDOW_MS`: flush window, default `200`
- `T3CODE_TRACE_MIN_LEVEL`: minimum trace level, default `Info`
- `T3CODE_TRACE_TIMING_ENABLED`: enable timing metadata, default `true`

OTLP export:

- `T3CODE_OTLP_TRACES_URL`: OTLP trace endpoint
- `T3CODE_OTLP_METRICS_URL`: OTLP metric endpoint
- `T3CODE_OTLP_EXPORT_INTERVAL_MS`: export interval, default `10000`
- `T3CODE_OTLP_SERVICE_NAME`: service name, default `t3-server`

If the OTLP URLs are unset, local tracing still works and metrics stay in-process only.

### What Is Instrumented Today

Current high-value span and metric boundaries include:

- Effect RPC websocket request spans from `effect/rpc`
- RPC request metrics in `apps/server/src/observability/RpcInstrumentation.ts`
- startup phases
- orchestration command processing
- orchestration command acknowledgment latency
- provider session and turn operations
- git command execution and git hook events
- terminal session lifecycle
- sqlite query execution

### Current Constraints

- logs outside spans are not persisted in the trace file; SSH-managed launch stdout/stderr is still
  captured in its launcher log
- metrics are not snapshotted locally
- the old `serverLogPath` still exists in config for compatibility, but the trace file is the primary
  structured persisted artifact
