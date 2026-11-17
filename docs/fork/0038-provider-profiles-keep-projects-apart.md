# 0038: Provider profiles keep projects apart

- PR: not opened yet
- Status: active

## What you can do now

- Name a set of allowed provider instances once, such as Personal or one per company, and assign
  it to a project instead of switching providers on and off project by project.
- Trust that a project only runs on its profile's instances. Anything else is left out of the
  model picker and refused by the server, including turns the server starts on its own.
- Add a provider instance without it leaking into existing work. A new instance belongs to no
  profile until you add it, and removing an instance removes it from every profile.
- Start new projects safely. Once a profile exists, a new project cannot run a provider until it
  is assigned one. Projects from before the first profile keep working as they did until you
  assign them.
- Manage profiles and assignments from settings on web, desktop, and mobile.

## Why

Some work cannot share models or accounts across companies for legal reasons. Per-project
provider switches made that a checklist to repeat for every new project and every new provider,
and one missed switch was enough to break the separation. A profile states the boundary once and
the project inherits it.

## Upstream considerations

Worth proposing as an idea first: upstream already has per-project provider switches, and a
profile is a named, reusable form of the same rule. The open product questions are whether a
project with no profile should be blocked once any profile exists, and whether a profile should
follow a thread delegated to another environment.

The rebase burden is moderate. The profile data lives in the fork's own settings block, but the
rule is checked wherever a provider is resolved and where a turn starts, and upstream changes
those paths often. A sync that adds a new way to start a turn or pick a provider needs the profile
check carried to it.
