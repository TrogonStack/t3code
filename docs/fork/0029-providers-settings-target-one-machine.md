# 0029: Provider settings say they apply to one machine

- PR: [TrogonStack/t3code#94](https://github.com/TrogonStack/t3code/pull/94)
- Upstream: [pingdotgg/t3code#17879](https://github.com/pingdotgg/t3code/pull/17879)
- Status: submitted

## What you can do now

- Open the Providers page and see only the machine its changes apply to. The
  project picker no longer appears there, so turning a provider off no longer
  looks like it affects a single project when it changes the whole machine.
- Arrive at Providers from a project or checkout link and land on the machine
  that project or checkout lives on.
- Switch machines on Providers and keep the project chosen for the other
  settings pages.

## Why

Provider configuration belongs to a machine, but the page offered a project
picker that suggested otherwise. Disabling a provider "for a project" quietly
disabled it everywhere on that machine, which is exactly the surprise the
scope sentence exists to prevent.

## Upstream considerations

Submitted upstream as is. Once it merges, the divergence and this entry go.
The change is confined to the settings scope picker, so the rebase burden is
small.
