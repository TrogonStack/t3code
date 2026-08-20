# 0016: Provider secrets can live in 1Password

- PR: [TrogonStack/t3code#27](https://github.com/TrogonStack/t3code/pull/27)
- Status: active

## What you can do now

- Give a provider its credential without giving T3 Code the credential. Paste a
  1Password `op://` secret reference as an environment variable value on any
  provider instance, and T3 Code reads the value from the 1Password CLI when it
  starts the agent. What gets saved is the reference; the secret itself is
  never written to the settings file or to T3 Code's secret store.
- Unlock your vault once instead of all day. Each reference is read a single
  time and held in memory for the life of the server, so starting a thread,
  sending a message, and the background status check all reuse it. One approval
  covers every reference across every provider, so a machine running five
  providers on references costs the same single unlock that one provider does,
  both at startup and on a refresh.
- Rotate a secret and pick it up without restarting anything. Refreshing
  provider status in Settings goes back to 1Password and rebuilds only the
  providers that read from it. Threads already running keep working.
- Tell when a vault is locked. A reference T3 Code cannot read leaves the
  variable unset, so the provider reports as not authenticated instead of
  starting with a blank credential and failing on your first message.

## Why

Running several accounts of the same provider side by side means several
long-lived tokens, and every one of them had to be copied out of the password
manager and pasted into a second store to be usable. That is the copy nobody
rotates: it outlives the original, it has no expiry anyone tracks, and it sits
in a file that gets synced, backed up, and occasionally shared with a support
thread.

Pointing at the secret instead of duplicating it keeps one copy under the
policy that was chosen for it, and makes rotation a vault edit plus a refresh
rather than a hunt for everywhere the value was pasted.

The in-memory hold is what makes it usable rather than merely correct. Reading
the vault on every thread start turns a background biometric prompt into a
constant interruption, which is the kind of friction that ends with the token
pasted in plaintext again to make it stop.

## Upstream considerations

Worth proposing upstream, though it is a product decision rather than a bug
fix, so it may not be wanted in that shape. The 1Password-specific piece is
deliberately confined to one layer behind a resolver service, which is the
seam another secret store would plug into, so the argument upstream is about
whether to carry any secret-store integration at all rather than about
1Password specifically.

Rebase burden is moderate. It adds a call in each of the five drivers' `create`
and one method on the provider instance registry, so a sync that reshapes
driver creation or instance rebuilding will conflict. The conflicts are shallow
and repetitive: the driver change is the same two lines in all five files.
