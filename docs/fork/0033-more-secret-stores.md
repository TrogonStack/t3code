# 0033: More secret stores

- PR: [TrogonStack/t3code#106](https://github.com/TrogonStack/t3code/pull/106)
- Status: active

## What you can do now

- Read from OpenBao. An OpenBao provider reads over its HTTP API with one explicit token source:
  its own stored token, `BAO_TOKEN`, or the `bao` CLI login.
- Read from AWS Secrets Manager. An AWS provider reads through the `aws` CLI in one region, with
  an optional named profile, so credentials stay in the CLI's own configuration.
- Edit a provider in place. Change its region, address, mount, namespace, or token from the same
  dialog. The name stays, so variables stay pointed at it, though a reference written for the old
  mount or region may need updating.
- Pick a 1Password account. The provider dialog lists the accounts the `op` CLI knows, so one
  account can be chosen without typing its address.
- Keep settings saved by earlier fork builds. Flat provider entries and variables saved with the
  earlier source shapes are converted when settings load, so they still resolve with nothing to
  migrate.

## Why

Upstream reads secrets from 1Password only. People also run OpenBao and AWS Secrets Manager, and
naming the provider once and letting every variable point at it keeps connection details out of
each variable.

## Upstream considerations

The pieces are built on upstream's secret provider architecture: OpenBao and AWS are drivers
beside the 1Password one, so they could be proposed as separate drivers. The in-place edit and the
account picker are small additions to upstream's secret UI. The per-variable check button was
upstream's own by the time of the 2026-11-12 sync, so the fork now uses upstream's.

The rebase burden is small. New kinds are separate packages registered in one list, and the
conversion of earlier settings is one pass before settings decode. A sync that reshapes the secret
provider dialog or the settings secrets store needs the additions carried forward.
