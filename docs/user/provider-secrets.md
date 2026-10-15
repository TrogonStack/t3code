# Provider Secrets From A Secret Manager

This guide is for people who keep provider credentials in 1Password or OpenBao and would rather not
copy them into a second place. It applies to every provider: Codex, Claude, Cursor, Grok, and
OpenCode.

For provider setup itself, see [Codex](./providers-codex.md) and [Claude](./providers-claude.md).

## I Do Not Want To Paste My Token Into T3 Code

Add your secret manager as a named provider once, then point variables at it instead of pasting
values.

1. Open Settings, then Secrets, and choose Add provider.
2. Pick 1Password CLI or OpenBao, give it a name, and fill in its details. 1Password needs the
   account sign-in address, such as `my.1password.com`. OpenBao needs its address and mount, and
   optionally a namespace and a token. Check Use the bao CLI only if you want the server to run the
   `bao` command instead of calling the OpenBao HTTP API.
3. In a provider's Variables section, add the variable, choose your named provider as its source, and
   fill in the reference.

```text
Name:      CLAUDE_CODE_OAUTH_TOKEN
Source:    Work 1Password
Reference: op://Private/claude-code/credential
```

You can add as many providers as you need, for example two 1Password accounts and an OpenBao server.
Each variable names the one it reads from.

T3 Code reads the value from your secret manager right before it starts the agent, and hands the
resolved value to the agent process only. The reference is what T3 Code stores; the secret itself
never lands in your settings file or in T3 Code's secret store.

Plain values are used exactly as typed, so mixing literal variables and references on the same
provider is fine. A plain value is never read from a secret manager, even one that starts with
`op://`.

Variables saved earlier with a 1Password account as their source keep working. A sensitive `op://`
value is hidden from the app, so it cannot be recognized; add it again with a named provider as its
source.

## What Does A Reference Look Like

- **1Password:** `op://vault/item/field`. To copy one, open the item, use the field's overflow menu,
  and choose **Copy Secret Reference**.
- **OpenBao:** `apps/openai#api_key`. The path is inside the mount set on the provider, and the part
  after `#` is the key to read. A reference without a key cannot be resolved.

## What Do I Need Installed

What each store needs, on the machine running the T3 Code server.

- **1Password:** the [1Password CLI](https://developer.1password.com/docs/cli/get-started/) (`op`),
  signed in.
- **OpenBao:** a token, and network access to the OpenBao server. By default T3 Code calls the
  OpenBao HTTP API (KV v2). It uses the provider's Token field, else `BAO_TOKEN` on the server, else
  the token `bao login` saved in `~/.vault-token`. The `bao` CLI is only needed when Use the bao CLI
  is checked, and then it uses its own login unless the provider has a token.

Confirm it works from a normal shell first:

```bash
op read --account my.1password.com --no-newline "op://Private/claude-code/credential"
curl -H "X-Vault-Token: $BAO_TOKEN" https://bao.example:8200/v1/secret/data/apps/openai
```

If the command prints your secret, T3 Code can read it too. If it asks you to sign in, sign in first,
otherwise providers using references will start unauthenticated.

Remote and tunnelled setups resolve references on the server, not on the device you are looking at.
The secret manager has to be reachable from wherever `npx t3` or the desktop app is actually running.

## Do I Still Mark It Sensitive

No. A reference is not a secret, so there is nothing to protect by storing it as one, and variables
that read from a secret manager are always stored as written.

## How Often Does It Ask Me To Unlock

Once, and then not again until you ask for it.

Each reference is read one time and held in memory for the life of the server. Starting a thread,
sending a message, and the background provider status check all reuse the value that was already
read, so a locked vault prompts you once rather than every few minutes.

One unlock covers every reference T3 Code needs, across every provider. Starting the server and
refreshing provider status both read the whole set in a single request per 1Password account, so
five providers backed by references in one account cost the same one approval that one provider
does.

## I Rotated The Secret, How Do I Pick Up The New One

Refresh provider status in Settings.

Refresh drops everything that was held in memory and rebuilds the providers that use references, so
the next read goes back to the secret manager. Providers with only literal variables are left alone.

The 1Password CLI's own cache is never used, so a refresh cannot be answered with the old value. Skipping
it also means T3 Code never starts the CLI's background `op daemon`, which can hang behind a macOS
permission prompt when the server runs in the background.

Changing a provider under Settings, Secrets also makes the variables that use it read again.

Threads that are already running keep the process they were given. Work started after the refresh
uses the new value.

## The Provider Says It Is Not Authenticated

That is what a failed read looks like, and it is deliberate: when T3 Code cannot read a reference it
leaves the variable unset rather than passing an empty value the agent would misread as a real one.

Work through it in this order:

1. Run the matching command above by hand. Most failures are a locked vault, an expired login, or a
   typo in the reference.
2. Confirm the provider still exists under Settings, Secrets. A variable naming a removed provider
   cannot be read.
3. Confirm `op` (or `bao`, when Use the bao CLI is checked) is on the `PATH` of whoever launched the T3 Code server. A CLI installed only for
   your interactive shell is not always visible to a background service.
4. Refresh provider status once the underlying problem is fixed. A failed read is remembered exactly
   like a successful one, so nothing retries on its own.

The server log records which reference failed and what the CLI said about it.
