# Provider constraints

Orchestration records intent and state without knowing which provider runs a thread. Provider
protocols, account ownership, permissions, and capabilities belong at the
[adapter boundary](../../apps/server/src/provider/Services/ProviderAdapter.ts). Normalize there
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
See the [adapter](../../apps/server/src/provider/Layers/OpenCodeAdapter.ts).

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
there. Anything unproven stays manual-only but still reports the version gap. npm updates pin
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
no pending RPC response to send. Blocking questions still use the request/response path. The
[adapter](../../apps/server/src/provider/Layers/CodexAdapter.ts) distinguishes them; the
[decider](../../apps/server/src/orchestration/decider.ts) records an async answer and its user
message together.

An async question can outlive the turn or a server restart. The engine reads that request's
durable activity before resolving it because the in-memory command snapshot omits old activities.
Do not infer that a request has disappeared merely because it is outside the recent window.

Capabilities must describe what the provider can actually do. Antigravity can capture workspace
checkpoints but cannot roll back its conversation. The [checkpoint boundary](./overview.md#turn-completion-and-checkpoints)
therefore rejects revert before touching files. Native permission and question option IDs must
also survive normalization; a display label is not necessarily a valid reply.

## Attachments and stored history

Attachments live outside the project workspace. [ProviderService](../../apps/server/src/provider/Layers/ProviderService.ts)
puts their environment-local paths in turn input and lets adapters choose native input formats.
A path in the prompt does not grant filesystem access. Keep provider sandbox and approval rules
in force; copying uploads into the project to bypass them changes that boundary.

File attachments introduced a replay compatibility limit. Image-only clients cannot decode
file-bearing messages, and an image-only server can fail the entire environment's startup when
replaying one such event. Rollouts and downgrades must account for persisted history as well as
current client support.

## Secret references in provider environments

A provider instance's `environment` is merged into the child process env by
`mergeProviderInstanceEnvironment`, once per driver inside `create`. A value that starts with `op://`
is not passed through: it is a secret reference, and
[`ProviderSecretResolver`](../../apps/server/src/provider/Services/ProviderSecretResolver.ts) swaps
it for the value the 1Password CLI returns before the merge happens.

The parsing half lives in
[`ProviderSecretReference.ts`](../../apps/server/src/provider/ProviderSecretReference.ts) and knows
nothing about how a secret is fetched, so the registry can ask "does this instance read from a secret
store?" without depending on the resolver.
[`ProviderSecretResolverLive`](../../apps/server/src/provider/Layers/ProviderSecretResolverLive.ts)
is the half that shells out to `op read --no-newline`.

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

## Stalled prompt detection

`session/prompt` is a long-lived request: ACP agents answer it only once the whole turn is done, and
nothing in the protocol says how long that takes. An agent whose upstream connection dies mid-turn
never answers and never errors, so
[`AcpSessionRuntime`](../../apps/server/src/provider/acp/AcpSessionRuntime.ts) races the RPC against a
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

Model classification has its own [manifest constraints](./model-manifest.md). Assistant-reference
handling is documented under [citations](./assistant-citations.md).
