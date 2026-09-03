# Provider architecture

> For maintainers. Using T3 Code? See [docs/user](../user/).

A provider is the agent runtime that does the actual work. T3 Code supports several, and the
orchestration layer does not know which one is behind a thread.

## Built-in drivers

[`builtInDrivers.ts`][drivers] exports `BUILT_IN_DRIVERS` with five entries:

| Driver kind   | Driver source                           |
| ------------- | --------------------------------------- |
| `codex`       | [`Drivers/CodexDriver.ts`][codex]       |
| `claudeAgent` | [`Drivers/ClaudeDriver.ts`][claude]     |
| `cursor`      | [`Drivers/CursorDriver.ts`][cursor]     |
| `grok`        | [`Drivers/GrokDriver.ts`][grok]         |
| `opencode`    | [`Drivers/OpenCodeDriver.ts`][opencode] |

Each driver declares its `driverKind`, a `configSchema`, and a `create` function that builds an
adapter in a child scope. Adapter implementations live beside them in
`apps/server/src/provider/Layers/` (`CodexAdapter.ts`, `ClaudeAdapter.ts`, and so on) and conform to
[`ProviderAdapter.ts`][adapter]. Read the driver plus its adapter to see how a specific agent's
transport, config, and event shapes are mapped.

## Registry and routing

Two registries separate configuration from live processes:

- [`ProviderInstanceRegistry`][instances] keys configured instances by `ProviderInstanceId`. Creating
  one looks up the driver by `driverKind`, decodes `entry.config` with that driver's schema, opens a
  child scope, and calls `driver.create`.
- [`ProviderAdapterRegistry`][registry] resolves an instance ID to its live adapter via
  `getByInstance`.

[`ProviderService`][service] sits on top. It combines the adapter registry with the provider session
directory to route session and turn operations for a thread, so callers name a thread, not an agent.

`ProviderService.sendTurn` expands [assistant citations](./assistant-citations.md) into quoted
reference data before dispatching to any adapter. Bound user comments remain distinct from the quoted
assistant text. Persisted messages keep their serialized links.

Adding a driver means writing the driver plus adapter and adding it to `BUILT_IN_DRIVERS`. No
orchestration, contract, or client change is required for the common case.

### Grok health check

`checkGrokProviderStatus` never opens an ACP session. It runs `grok --version`, then `grok models`
for login state and model slugs, then a single ACP `initialize` and reads models from
`_meta.modelState`. `authenticate` and `session/new` are skipped on purpose: `authenticate` can open
a browser login and `session/new` boots every configured MCP server, both of which made background
probes hang or surprise the user. A failed `initialize` degrades to `warning` with the CLI's model
list instead of persisting `error` over a working install. The built-in `grok-build` slug is the
CLI's product name, not an ACP model id. `applyGrokAcpModelSelection` treats it as "keep the
session's current model" and never sends it in `session/set_model`.

## OpenCode server ownership and catalog

Each OpenCode provider instance owns one lazy local server for catalog discovery and
text-generation helpers through [`OpenCodeServerOwner.ts`][opencode-server-owner]. Concurrent
borrowers share startup. The server closes 30 seconds after the last borrower releases it, or
when the provider instance closes. A failed or exited process can be started again on the next
use. An externally configured OpenCode server remains externally owned.

The local server and its SDK clients use one resolved password. An explicit provider password
overrides `OPENCODE_SERVER_PASSWORD` in the spawned environment. Without an explicit password,
the client uses the password from the environment that the process inherits. External servers use
only their explicit provider password and never inherit the host's local password.

Every server connection must pass the authenticated `/global/health` check before inventory or
session operations start. The response must contain a valid version at or above 1.14.19. Local
owners cache this result for the lifetime of the spawned process. External actions check once when
they create their server connection, not for each model or SDK request.

Chat adapters keep their own server per thread. They register a thread-specific `t3-code` MCP
connection, while OpenCode stores MCP connections by directory. Sharing these chat servers
without changing MCP routing would let two threads in one directory replace each other's
connection.

OpenCode loads its catalog through the HTTP API when an enabled provider instance starts. The
provider registry keeps the snapshot in memory and persists it in the existing per-instance cache.
Each `subscribeServerConfig` connection refreshes all providers, so a client reconnect reloads the
OpenCode catalog from the current helper. The `serverRefreshProviders` request also refreshes it.
Periodic OpenCode probes remain disabled. OpenCode reads credentials for each inventory request,
but its native configuration files can remain cached for the lifetime of the helper process. The
helper closes 30 seconds after its last inventory or text-generation borrower releases it. A
refresh after that idle period starts a new helper and reads file changes. Repeated refreshes and
active text-generation work can extend process reuse. Changes to the provider configuration or
environment replace the instance and start a new discovery. Changes to unrelated settings only
update snapshot enrichment. Other providers retain their existing refresh policy.

T3 Code does not own an external OpenCode process. Native configuration changes there can require
an external reload or restart before T3 Code's next refresh sees them.

The shared server's idle shutdown does not clear the catalog. Failed discovery keeps the last
known models, slash commands, and skills through the registry's existing merge rules. A successful
empty inventory is authoritative. Existing threads keep their explicit model identifier and
options when catalog metadata is missing; the catalog is not permission to choose a different
model for a thread.

## Model manifest

The model picker's legacy section is driven by `apps/server/src/provider/model-manifest.json`, which
lists the current (non-legacy) model slugs per driver kind. The `ModelManifest` service
(`apps/server/src/provider/ModelManifest.ts`) refreshes that data from the same file on `main` via
raw.githubusercontent.com, so moving a model in or out of the legacy section is a commit, not a
release. Preference order is remote fetch, then the on-disk copy of the last successful fetch (in
the state directory), then the bundled copy. Fetches are TTL-gated, run concurrently with provider
probes, respect the `enableProviderUpdateChecks` setting, and never fail a provider check. The
Codex and Claude drivers apply the classification to every snapshot with `applyModelManifest`;
driver kinds absent from the manifest have no legacy concept.

## Attachment access

The server stores uploaded attachments in its attachment directory, outside the project workspace.
`ProviderService` adds the absolute path of each attachment to the turn text, then passes every
attachment to the provider adapter. Each adapter decides what its provider ingests natively:

- Codex, Claude, Cursor, and Grok send images as native image inputs and skip generic files. For
  these providers, generic files reach the agent only as file paths in the turn text.
- OpenCode sends PNG/JPEG/GIF/WebP images, text files, and PDFs up to 20 MB as native file parts
  with their real mime type. Everything else (ZIP and other binaries, image formats model APIs
  reject, oversized files) falls back to the file path in the turn text, like the other providers.

Claude receives the attachment directory as an allowed additional directory. Codex keeps its
configured sandbox policy, so access depends on that policy and the selected runtime mode. OpenCode
allows all paths in full-access mode and requests approval for directories outside the workspace in
restricted modes. Cursor and Grok use their own provider permission rules.

The server does not copy attachments into a project or bypass provider approval rules. If an agent
cannot read an attachment, the user must approve the access or select a runtime mode that permits it.

Updated attachment schemas tolerate unknown attachment members, but old image-only clients still
cannot decode messages that contain file attachments. Client file-picking rollouts must account for
this limit.

Do not run an old image-only server against state that contains file attachments. Replay decodes
each persisted event before projection. A file-bearing event can make `ProjectionPipeline` bootstrap
and `OrchestrationEngine` startup fail for the entire environment, not only the affected thread.

## Secret references in provider environments

A provider instance's `environment` is merged into the child process env by
`mergeProviderInstanceEnvironment`, once per driver inside `create`. A value that starts with `op://`
is not passed through: it is a secret reference, and [`ProviderSecretResolver`][secretresolver]
swaps it for the value the 1Password CLI returns before the merge happens.

The parsing half lives in [`ProviderSecretReference.ts`][secretref] and knows nothing about how a
secret is fetched, so the registry can ask "does this instance read from a secret store?" without
depending on the resolver. [`ProviderSecretResolverLive`][secretlive] is the half that shells out to
`op read --no-newline`.

Three decisions are load-bearing:

- **A failed read unsets the variable.** It never substitutes an empty string, because an empty
  `ANTHROPIC_API_KEY` reads to the provider as a configured-but-broken credential rather than an
  absent one, and the status badge would go back to lying about it. Unsetting is stronger than
  leaving the name out of the resolved list: the child environment starts from the server's own, so
  a name left alone keeps whatever the server inherited under it, and the agent would quietly run
  as a different account than the one the instance names.
- **Reads are batched across instances, and sequential within one.** The store charges an unlock per
  `op` invocation, not per secret, and every instance resolves its own environment inside `create`,
  so a fleet would otherwise cost one prompt per provider. `prime` reads the whole set in a single
  `op inject` before the builds start, called from the settings watcher (which covers boot) and from
  `reloadSecretBackedInstances` (which covers the refresh button). Whatever `prime` misses,
  `resolve` still walks with a plain loop rather than `Effect.forEach` with concurrency, so it
  produces one prompt rather than several simultaneous ones.
- **Priming can only ever help.** It is best effort and never fails: `op inject` resolves the whole
  template or fails it, so one bad reference would take the batch down with it. A batch that does
  not come back leaves the cache exactly as cold as it found it and `resolve` falls back to reading
  one reference at a time, which is both where the per-variable failure isolation lives and how the
  user learns which reference is the broken one. A separator is generated per call because a secret
  can contain anything, newlines included.
- **Failures are cached alongside successes.** The `Cache` holds Exits, so a locked vault costs one
  prompt per refresh cycle instead of one per thread start. Recovery is the refresh button, not a
  timeout: the cache is built without a `timeToLive`.

### Why a refresh has to rebuild the instance

A driver resolves its environment once, at `create` time, and `makeManagedServerProvider` re-probes
using that captured `processEnv`. Dropping the cached secret therefore changes nothing on its own,
because the running instance still holds the value it was built with.

So `ProviderRegistry.reloadSecretBackedInstances` invalidates the cache and then calls
[`rebuildInstanceWhen`][instances] on each instance, passing `hasProviderSecretReference` as the
predicate. The registry owns the child scopes and already stores each instance's config, so it can
close and rebuild one entry in place; passing a predicate rather than exposing the entries keeps
secret-store policy in `ProviderRegistry` and keeps the instance registry ignorant of 1Password.
`reconcile` cannot do this job, because it diffs the config envelope and a rotated secret leaves
that envelope byte-identical.

A rebuild takes as long as the secret read does, and a locked vault can park it on a person at a
biometric prompt. Three things follow from that window being long:

- **The instance leaves the map before its scope closes.** Otherwise every lookup for the whole
  window hands back a bundle whose scope is already gone. Its last snapshot stands in for it
  meanwhile, because `ProviderRegistry` prunes ids it finds in neither list, and the card must not
  blink out of Settings while 1Password waits on a fingerprint.
- **Rebuilds and `reconcile` take turns.** Both read the instance map, do slow work, then write it
  back, so a settings change landing mid-rebuild would be overwritten by the map the rebuild read
  before it. Reads stay outside the lock: `getInstance` and `listInstances` never wait on a
  1Password prompt.
- **A rebuild that fails is retryable.** The registry keeps the config envelope of an instance it
  could not bring back, so the next refresh retries it, and a refresh with no explicit target
  covers the unavailable instances as well as the live ones. Without both halves, a vault that
  happened to be locked would cost the user the instance until settings changed. The envelope is
  recorded before the build starts and the map writes around the build are uninterruptible, so a
  refresh whose caller walked away mid-read is retryable on the same terms as one that failed.

This hangs off the three refresh entry points (`refreshAll`, the kind-scoped `refresh`, and
`refreshInstance`), all of which are reached only by a user action: the Settings refresh button, and
the post-update verification in `providerMaintenanceRunner`. The periodic provider health loop is
not one of them. It lives inside `makeManagedServerProvider` and calls `refreshSnapshot` directly,
which is what keeps a resolved secret alive between refreshes instead of re-reading it every few
minutes.

## Claude credential liveness

The Claude capability probe reports which credential the CLI _found_, which is a different question
from whether Anthropic still honours it. A revoked or expired setup token still reports
`tokenSource: "CLAUDE_CODE_OAUTH_TOKEN"`, so Settings kept showing a green badge until the first
turn failed.

[`ClaudeCredential.ts`][claudecred] closes that gap with one authenticated `GET /v1/models`, chosen
because it is the cheapest first-party endpoint that accepts the token and costs no message quota.
The status check runs it from the same path that already spawns the probe, so it inherits the
provider health cadence and needs no cache of its own.

Four decisions carry the behavior:

- Only a value shaped like an Anthropic credential is ever sent: the `sk-ant-` prefix followed by
  key material and nothing else. A placeholder, or a secret reference nothing resolved, earns a
  `401` that says nothing about the token, and acting on it would blame the credential for a
  problem one layer up. The tail is checked because a placeholder can wear the prefix
  (`sk-ant-oat01-${MY_TOKEN}`), and rejecting a real token by mistake only costs an `unknown`.
- Only an explicit `401` counts. Timeouts, transport errors, `403`, and every 5xx answer `unknown`,
  because a proxy or an outage must never sign a working install out of Settings.
- The check only ever downgrades. It runs after the probe has already concluded the instance is
  authenticated, so it can turn a green badge red but never the reverse.
- It is gated on the CLI reporting that it took its token from the environment. An install that
  authenticates through a keychain login, an API key, a router, or a cloud backend is never judged
  by a variable it ignores, even when that variable happens to be set.

Note that Bearer authentication with a Claude Code OAuth token is not part of Anthropic's public API
surface. It is verified working, not contractually stable, which is the other reason every
unexpected answer is treated as `unknown`.

## How provider work is requested

Clients never call a provider directly. They dispatch orchestration commands over the RPC method
`orchestration.dispatchCommand`, defined with the rest of the orchestration surface in
[`orchestration.ts`][contracts]. The client-dispatchable provider-facing commands are
`thread.turn.start`, `thread.turn.interrupt`, `thread.approval.respond`,
`thread.user-input.respond`, `thread.checkpoint.revert`, and `thread.session.stop`, plus the mode
setters `thread.runtime-mode.set` and `thread.interaction-mode.set`.

The engine persists an event for the command, and a server-side reactor performs the provider call.
Provider output comes back as internal commands such as `thread.message.assistant.delta` and
`thread.session.set`, which clients observe through `orchestration.subscribeThread`. See
[overview.md](./overview.md) for the command/event loop.

## Server-side workers

Provider work flows through three queue-backed workers. All three are built with
`makeDrainableWorker` from [`DrainableWorker.ts`][worker] and expose `drain` for deterministic test
synchronization.

1. [`ProviderRuntimeIngestion`][ingest] consumes provider runtime streams and emits orchestration
   commands.
2. [`ProviderCommandReactor`][cmd] reacts to orchestration intent events and dispatches provider
   calls.
3. [`CheckpointReactor`][checkpoint] captures workspace checkpoints on turn start and completion, and
   performs reverts.

### Buffered assistant delivery

A thread in `buffered` assistant delivery mode accumulates assistant text instead of streaming each
delta. The buffer is not held until turn completion. In [`ProviderRuntimeIngestion`][ingest],
`MAX_BUFFERED_ASSISTANT_CHARS` is 24,000: the append that would exceed it invalidates the buffer and
spills the whole accumulated text as one delta. The buffer also flushes at interaction boundaries,
when a request opens (approval) or user input is requested, via
`flushBufferedAssistantMessagesForTurn`.

### Stalled prompt detection

`session/prompt` is a long-lived request: ACP agents answer it only once the whole turn is done, and
nothing in the protocol says how long that takes. An agent whose upstream connection dies mid-turn
never answers and never errors, so [`AcpSessionRuntime`][acpruntime] races the RPC against a
liveness watchdog and fails the turn instead of waiting forever.

Liveness is traffic for this session plus outstanding work, and both halves are load-bearing.

Scoping matters because one runtime projects one root session: a child session chattering on the
same pipe says nothing about whether the root prompt is alive, so only updates that pass the
root-session check refresh the stamp. Counting outstanding requests matters because silence is
often our fault, not the agent's. Every request the agent makes of us is held open while it runs,
including the extension requests, which is the case worth stating: `cursor/ask_question` and
`x.ai/ask_user_question` park on a human, and a user who takes fifteen minutes to answer must not
look like a dead agent. The same holds for a twenty-minute `terminal/wait_for_exit`.

A stall therefore needs no root-session traffic _and_ nothing of ours outstanding, for
`promptStallTimeout` (ten minutes by default).

On a stall the runtime sends `session/cancel` so the agent can release the dead prompt and stay
usable, then fails with an `AcpTransportError`. `ProviderCommandReactor` turns that into a thread
session error with a `provider.turn.start.failed` activity and clears `activeTurnId`, so the working
indicator stops and the reason is visible in the timeline.

[secretref]: ../../apps/server/src/provider/ProviderSecretReference.ts
[secretresolver]: ../../apps/server/src/provider/Services/ProviderSecretResolver.ts
[secretlive]: ../../apps/server/src/provider/Layers/ProviderSecretResolverLive.ts
[drivers]: ../../apps/server/src/provider/builtInDrivers.ts
[codex]: ../../apps/server/src/provider/Drivers/CodexDriver.ts
[claude]: ../../apps/server/src/provider/Drivers/ClaudeDriver.ts
[claudecred]: ../../apps/server/src/provider/Drivers/ClaudeCredential.ts
[cursor]: ../../apps/server/src/provider/Drivers/CursorDriver.ts
[grok]: ../../apps/server/src/provider/Drivers/GrokDriver.ts
[opencode]: ../../apps/server/src/provider/Drivers/OpenCodeDriver.ts
[opencode-server-owner]: ../../apps/server/src/provider/OpenCodeServerOwner.ts
[adapter]: ../../apps/server/src/provider/Services/ProviderAdapter.ts
[instances]: ../../apps/server/src/provider/Services/ProviderInstanceRegistry.ts
[registry]: ../../apps/server/src/provider/Services/ProviderAdapterRegistry.ts
[service]: ../../apps/server/src/provider/Layers/ProviderService.ts
[contracts]: ../../packages/contracts/src/orchestration.ts
[worker]: ../../packages/shared/src/DrainableWorker.ts
[ingest]: ../../apps/server/src/orchestration/Layers/ProviderRuntimeIngestion.ts
[acpruntime]: ../../apps/server/src/provider/acp/AcpSessionRuntime.ts
[cmd]: ../../apps/server/src/orchestration/Layers/ProviderCommandReactor.ts
[checkpoint]: ../../apps/server/src/orchestration/Layers/CheckpointReactor.ts
