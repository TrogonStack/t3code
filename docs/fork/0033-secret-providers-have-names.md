# 0033: Secret providers have names

- PR: [TrogonStack/t3code#106](https://github.com/TrogonStack/t3code/pull/106)
- Status: active

## What you can do now

- Connect more than one secret store and give each a name. Under Settings,
  Secrets, add a provider for the 1Password CLI or for OpenBao, and keep as many
  as you need side by side.
- Pick a provider by name on any provider variable. The variable keeps only the
  provider name and the reference inside it, so switching a variable to another
  store is a change of source, not a rewrite.
- Read from OpenBao as well as 1Password. An OpenBao provider reads over its HTTP API
  with one explicit token source: its own stored token, `BAO_TOKEN`, or the `bao` CLI login.
- Read from AWS Secrets Manager. An AWS provider reads through the `aws` CLI in one region, with
  an optional named profile, so credentials stay in the CLI's own configuration.
- Read a provider's own setup fields, such as the Cursor API key, from a secret provider. Pick the
  provider next to the field instead of adding the same variable by hand under Environment.
- Edit a provider in place. Change its region, address, mount, namespace, or token from the same
  dialog. The name stays, so variables stay pointed at it, though a reference written for the old
  mount or region may need updating.
- Check a reference before saving it. A check button on each secret variable reads the reference
  through its provider right now and shows whether it resolved, with the store's error when it did
  not.
- Keep variables saved with the earlier 1Password source. They still resolve and
  still open as before, with nothing to migrate.

## Why

Reading credentials from 1Password solved one store for one account. People
also run OpenBao, and some work across several 1Password accounts, so a variable
that hard-codes "1Password plus an account" stops fitting. Naming the provider
once, in one place, lets every variable point at it and keeps connection details
out of each variable.

## Upstream considerations

Upstream now carries a first slice of named secret providers: settings-only
1Password providers stored as a kind, a label, and a kind-specific config. It
does not read secrets from them yet and has no OpenBao or AWS kinds. The fork
accepts both shapes. Providers it already stores keep their flat shape and
behave as before, and providers written in upstream's shape are listed and read
from the same way, so settings move freely between the fork and upstream.

The flat kinds are a pending convergence: once upstream resolves secrets and
adds more kinds, the fork should store its providers in upstream's shape and
drop the flat one. Until then, the Secrets page is always on in the fork,
where upstream gates it behind a feature flag.

Beyond that, this builds on the secret-store integration we already carry
(0016) and shares its argument: whether to carry any secret-store integration
at all. A new kind of store is a new kind of provider, not a new concept.

The rebase burden is small beyond what 0016 already costs. It adds a settings
section and a page, so a sync that reshapes settings navigation or the provider
variables editor needs the new pieces carried forward.
