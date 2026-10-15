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
- Edit a provider in place. Change its region, address, mount, namespace, or token from the same
  dialog, and the variables that read from it keep working because the name stays.
- Keep variables saved with the earlier 1Password source. They still resolve and
  still open as before, with nothing to migrate.

## Why

Reading credentials from 1Password solved one store for one account. People
also run OpenBao, and some work across several 1Password accounts, so a variable
that hard-codes "1Password plus an account" stops fitting. Naming the provider
once, in one place, lets every variable point at it and keeps connection details
out of each variable.

## Upstream considerations

Upstream has no secret-store integration, so this builds on the one we already
carry (0016) and shares its argument: whether to carry any secret-store
integration at all. A new kind of store is a new kind of provider, not a new
concept.

The rebase burden is small beyond what 0016 already costs. It adds a settings
section and a page, so a sync that reshapes settings navigation or the provider
variables editor needs the new pieces carried forward.
