# Provider Secrets From 1Password

This guide is for people who keep provider credentials in 1Password and would rather not copy them
into a second place. It applies to every provider: Codex, Claude, Cursor, Grok, and OpenCode.

For provider setup itself, see [Codex](./providers-codex.md) and [Claude](./providers-claude.md).

## I Do Not Want To Paste My Token Into T3 Code

Paste the 1Password reference instead of the value.

In the provider's Environment variables section in Settings, use the `op://` secret reference as the
value:

```text
Name:  CLAUDE_CODE_OAUTH_TOKEN
Value: op://Private/claude-code/credential
```

T3 Code reads the value with the 1Password CLI right before it starts the agent, and hands the
resolved value to the agent process only. The reference is what T3 Code stores; the secret itself
never lands in your settings file or in T3 Code's secret store.

Any value beginning with `op://` is treated this way. Everything else is used exactly as typed, so
mixing literal variables and references on the same provider is fine.

To copy a reference in 1Password, open the item, use the field's overflow menu, and choose
**Copy Secret Reference**.

## What Do I Need Installed

The [1Password CLI](https://developer.1password.com/docs/cli/get-started/), signed in on the machine
running the T3 Code server.

Confirm it works from a normal shell first:

```bash
op read --no-newline "op://Private/claude-code/credential"
```

If that command prints your secret, T3 Code can read it too. If it asks you to sign in, sign in
first, otherwise providers using references will start unauthenticated.

Remote and tunnelled setups resolve references on the server, not on the device you are looking at.
The vault has to be reachable from wherever `npx t3` or the desktop app is actually running.

## Do I Still Mark It Sensitive

You do not need to. A reference is not a secret, so there is nothing to protect by storing it as one.

Marking it sensitive still works if you prefer the redacted field in the UI, and the reference is
resolved the same way either way.

## How Often Does It Ask Me To Unlock

Once, and then not again until you ask for it.

Each reference is read one time and held in memory for the life of the server. Starting a thread,
sending a message, and the background provider status check all reuse the value that was already
read, so a locked vault prompts you once rather than every few minutes.

Providers with more than one reference are read one after another, so a single unlock covers all of
them.

## I Rotated The Secret, How Do I Pick Up The New One

Refresh provider status in Settings.

Refresh drops everything that was held in memory and rebuilds the providers that use references, so
the next read goes back to 1Password. Providers with only literal variables are left alone.

Threads that are already running keep the process they were given. Work started after the refresh
uses the new value.

## The Provider Says It Is Not Authenticated

That is what a failed read looks like, and it is deliberate: when T3 Code cannot read a reference it
leaves the variable unset rather than passing an empty value the agent would misread as a real one.

Work through it in this order:

1. Run the `op read` command above by hand. Most failures are a locked vault or a typo in the
   reference.
2. Confirm `op` is on the `PATH` of whoever launched the T3 Code server. A CLI installed only for
   your interactive shell is not always visible to a background service.
3. Refresh provider status once the underlying problem is fixed. A failed read is remembered exactly
   like a successful one, so nothing retries on its own.

The server log records which reference failed and what the 1Password CLI said about it.

## Can I Use A Different Password Manager

Not yet. `op://` references are the only form T3 Code resolves today.
