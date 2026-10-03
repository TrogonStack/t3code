# Provider constraints

Orchestration records intent and state without knowing which provider runs a thread. Provider
protocols, account ownership, permissions, and capabilities belong at the
[adapter boundary](../../apps/server/src/orchestration-v2/ProviderAdapter.ts). Normalize there
instead of spreading provider checks through reactors and clients.

A driver kind identifies an integration; an instance identifies one configuration and account
lifecycle. Route work by instance, so two accounts using the same driver do not share mutable
session or catalog state.

## Process and account isolation

T3-managed OpenCode chat uses one server per thread. Its MCP registrations are directory-scoped, while
T3's MCP connection is thread-scoped. Sharing a chat server between threads in one directory would
let them replace each other's connection. Catalog and text-generation work can share the
[instance-owned helper](../../apps/server/src/provider/OpenCodeServerOwner.ts), which closes
after an idle period. External OpenCode servers remain externally owned and can require an
external restart to pick up configuration changes.

OpenCode also stores persistent approval grants per directory. Automatic full-access replies use
`once` so they cannot widen a supervised thread's permissions on a shared external server.
See the [adapter](../../apps/server/src/orchestration-v2/Adapters/OpenCodeAdapterV2.ts).

Pi runs the user's own `pi` install in RPC mode and owns native extension, package, and project
trust discovery. T3 injects only its namespaced MCP bridge, so a Pi session behaves as it does in
the Pi TUI. Pi session files back native resume, rollback, and same-instance thread forks.
Forks use Pi's CLI in the destination directory because RPC session switching retains the source
session's cwd. Provider switches still use portable handoff summaries.
See the [adapter](../../apps/server/src/orchestration-v2/Adapters/PiAdapterV2.ts).

Antigravity separates account profiles per instance while sharing installed executables across the
environment. It forces file-based credential storage because the native macOS keychain entry would
otherwise be shared across instances. The launch environment removes ambient Google credentials,
so an instance cannot silently use another account or billing project. The agent also resolves
its user-global skill directories under that profile, so the profile links those two directories
back to the user's real `~/.gemini`; MCP servers, hooks, and rules there stay out of the profile.
See [profile isolation](../../apps/server/src/provider/antigravityAuthSupport.ts).

The [Antigravity installer](../../apps/server/src/provider/AntigravityInstallation.ts) outlives
client connections and provider-instance rebuilds. Releases are immutable, with an atomic pointer
selecting the version for new processes. Running processes hold leases on their version. Updates
and removal must respect those leases instead of replacing executables under a running agent.

## Setup must not happen as a health-check side effect

Opening a provider session can start MCP servers, run hooks, or launch a login browser.
[Grok probes](../../apps/server/src/provider/Layers/GrokProvider.ts) avoid authentication and
session creation for this reason. Antigravity likewise reserves authenticated catalog sessions for
explicit setup or model refresh; background checks use initialization only.

[Antigravity sign-in](../../apps/server/src/provider/AntigravityAuth.ts) belongs to the initiating
T3 auth session. The client carries the return URL back to the environment because the provider's
loopback listener may be on another machine. Forward only the callback for the owned pending flow;
a successful callback HTTP request is not proof that provider authentication finished. The native
process owns token exchange and storage.

Managed ChatGPT sign-in for a remote environment can finish on a local primary. The
[primary handoff](../../apps/server/src/provider/CodexChatGptHandoff.ts) uses an ephemeral
credential store and the destination's environment ID. It exchanges and verifies the code before
transferring the issued client registration and tokens. Only the destination persists and refreshes
that session; retaining a primary refresh session would race refresh-token rotation. Without a local
primary, the client uses the remote callback completion flow.

Antigravity sign-out closes admission to new processes and stops existing processes before clearing account
metadata. Otherwise a helper or resumed session could retain the old account. Cached model lists
do not establish current access, and an authoritative empty catalog must clear the old list.

Antigravity text-generation helpers deny tool requests, but native hooks and MCP configuration can
run before the prompt. They reject profiles with such configuration before launch. Prompt
instructions and tool denial do not create a native sandbox.
See [helper constraints](../../apps/server/src/textGeneration/AntigravityTextGeneration.ts).

## Provider updates run only through the owning installer

A one-click update is offered only when the resolved executable's path proves which installer owns
it. Homebrew and npm are proven by the real path (symlinks followed): a versioned keg or cask under
`brew --prefix`, or `<prefix>/lib/node_modules/<pkg>/` (Windows: the shim beside `node_modules`).
Native installer layouts and the global bin directories of pnpm, Bun, and Vite+ may match on either
the resolved path or its real target, since those installers place real files or their own symlinks
there. Cursor and Grok are the exception: their only updater is the CLI itself, which detects its
own installer, so any resolved executable runs `<binary> update`. Anything unproven stays
manual-only but still reports the version gap. npm updates pin
`--prefix` because the `npm` on `PATH` can belong to a different Node than the one that owns the
provider. Homebrew
compares against `brew info` since casks trail npm by hours; native installs share npm's version
train, so the registry stays authoritative for them.
See the [resolver](../../apps/server/src/provider/providerMaintenance.ts).

Ownership is cached per instance and re-read immediately before an update runs. The
[runner](../../apps/server/src/provider/providerMaintenanceRunner.ts) refuses when the lock key
changed since the advisory, and reports success only when the refreshed provider is still installed
with a readable, current version.

## Protocol traps

Codex async questions arrive as notifications and are answered with a new user message. There is
no pending RPC response to send. The
[adapter](../../apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts) persists them as
`user_input_request` turn items and runtime requests with `responseCapability: { type: "message" }`.
Their execution nodes do not block the run. Web, desktop, and mobile use their normal question
panels, and requests remain pending after a turn finishes, a provider exits, or the server restarts.

`runtime-request.respond` reads the persisted request and question item, validates required
answers, and commits the resolution and a user message in one transaction. Repeating the same
command returns its receipt without posting the answer twice. The normal message path starts or
resumes a run, queues behind active work, or steers when the adapter supports it. Blocking questions
retain the provider's live response path. Do not infer that a request has disappeared merely because
it is outside the recent history window.

Capabilities must describe what the provider can actually do. Antigravity can capture workspace
checkpoints but cannot roll back its conversation. The [checkpoint boundary](./overview.md#turn-completion-and-checkpoints)
therefore rejects revert before touching files. Native permission and question option IDs must
also survive normalization; a display label is not necessarily a valid reply.

## Attachments and stored history

Attachments live outside the project workspace. The
[attachment boundary](../../apps/server/src/orchestration-v2/AttachmentClaims.ts) validates and claims
uploads for a thread; adapters choose native input formats for those environment-local files.
A path in the prompt does not grant filesystem access. Keep provider sandbox and approval rules
in force; copying uploads into the project to bypass them changes that boundary.

File attachments introduced a replay compatibility limit. Image-only clients cannot decode
file-bearing messages, and an image-only server can fail the entire environment's startup when
replaying one such event. Rollouts and downgrades must account for persisted history as well as
current client support.

## Secret references in provider environments

A provider instance's `environment` can hold secret references: a `ProviderSecretSource` value such
as `{ kind: "1password", reference, account }`, or a legacy plain string starting with `op://` that
reads from the CLI's default account.
[`ProviderSecretResolver`](../../apps/server/src/provider/Services/ProviderSecretResolver.ts) swaps
each one for the value the 1Password CLI returns. This happens once per instance in
[`ProviderInstanceRegistryLive`](../../apps/server/src/provider/Layers/ProviderInstanceRegistryLive.ts),
before `driver.create`, so drivers and the orchestration v2 adapters built from them only ever see
resolved values; `ResolvedProviderEnvironment` is the type that enforces it. The registry keeps the
raw config, which is what makes a later rebuild possible. Paths that build a process environment
without the resolver (terminals, usage, installation, session scanning) use
`literalProviderInstanceEnvironment`, which leaves references out rather than passing them through.

The parsing half lives in
[`ProviderSecretReference.ts`](../../apps/server/src/provider/ProviderSecretReference.ts) and knows
nothing about how a secret is fetched, so the registry can ask "does this instance read from a secret
store?" without depending on the resolver.
[`ProviderSecretResolverLive`](../../apps/server/src/provider/Layers/ProviderSecretResolverLive.ts)
is the half that shells out to `op read --account <account> --no-newline`. The account is part of the
cache key, because the same reference in two accounts is two different secrets.

Three decisions are load-bearing:

- **A failed read unsets the variable.** It never substitutes an empty string, because an empty
  `ANTHROPIC_API_KEY` reads to the provider as a configured-but-broken credential rather than an
  absent one, and the status badge would go back to lying about it. Unsetting is stronger than
  leaving the name out of the resolved list: the child environment starts from the server's own, so
  a name left alone keeps whatever the server inherited under it, and the agent would quietly run
  as a different account than the one the instance names. The registry therefore hands the driver a
  `HostProcessEnvironment` with every unresolved name removed.
- **Reads are batched across instances, and sequential within one.** The store charges an unlock per
  `op` invocation, not per secret, and every instance resolves its own environment as it is built,
  so a fleet would otherwise cost one prompt per provider. `prime` reads the whole set with one
  `op inject` per account before the builds start, called from the settings watcher (which covers boot) and from
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

An instance's environment is resolved once, when the registry builds it, and
`makeManagedServerProvider` re-probes using that captured `processEnv`. Dropping the cached secret
therefore changes nothing on its own, because the running instance still holds the value it was
built with. Sessions follow a rebuild without extra wiring, because the v2 adapter registry looks up
the instance's adapter on every request.

So `ProviderRegistry.reloadSecretBackedInstances` invalidates the cache and then calls
[`rebuildInstanceWhen`](../../apps/server/src/provider/Services/ProviderInstanceRegistry.ts) on each
instance, passing `hasProviderSecretReference` as the predicate. The registry owns the child scopes
and already stores each instance's config, so it can
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

[`ClaudeCredential.ts`](../../apps/server/src/provider/Drivers/ClaudeCredential.ts) closes that gap with one authenticated `GET /v1/models`, chosen
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

## Provider diagnostics

Native event logs retain lifecycle events, responses, and failures. Token deltas and duplicate raw
frames are filtered before adapters copy or redact payloads. The filter accepts both legacy native
events and v2 protocol envelopes; decode failures remain visible through diagnostic frames.

Log payloads have a 64 KiB encoded budget. Large or deeply nested payloads become structural
summaries that retain routing identifiers, methods, status, and error fields. Traversal is bounded
before redaction and serialization, so logging a large response does not require several full
copies. These limits apply to diagnostics; provider event handling is unchanged.

Codex resumes with metadata-only reads when it needs a thread's identity and update time. Its
initialization capabilities opt out of `turn/diff/updated`: T3 derives diffs from checkpoints.
The logger filters those notifications before traversal when an older provider still sends them.

Model classification has its own [manifest constraints](./model-manifest.md). Assistant-reference
handling is documented under [citations](./assistant-citations.md).
