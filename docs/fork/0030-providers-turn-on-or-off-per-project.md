# 0030: Providers can be turned on or off per project

- PR: [TrogonStack/t3code#96](https://github.com/TrogonStack/t3code/pull/96)
- Upstream: [pingdotgg/t3code#17879](https://github.com/pingdotgg/t3code/pull/17879)
- Status: submitted

## What you can do now

- Pick a project on the Providers settings page and turn a provider on or off
  for that project alone. A provider off on the machine can still be used in
  one project, and one on everywhere can be kept out of another.
- Reset a project back to following the machine's setting.
- Trust the choice everywhere a turn can start: that project's model pickers
  on web, desktop and mobile follow it, and new messages from agents or
  scheduled tasks cannot use a provider the project turned off.

## Why

The Providers page let you pick a project, but the switch changed the whole
machine, so turning a provider off for one repository quietly turned it off
everywhere. A maintainer confirmed that per-project enablement is the intended
behavior, so the page now does what it implies instead of hiding the project.

## Upstream considerations

Submitted upstream on the same thread that first proposed hiding the project
picker. Once it merges, the divergence and this entry go. Only enablement is
per project; every other provider setting stays per machine, which keeps the
rebase burden to the settings schema, the turn start gate and the Providers
page.
