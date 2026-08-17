# 0008: Drop a folder on the sidebar to add a project

- PR: [TrogonStack/t3code#17](https://github.com/TrogonStack/t3code/pull/17)
- Status: active

## What you can do now

- Drag a folder from your file manager onto the desktop app's sidebar to add it
  as a project. The add-project surface opens with that folder filled in, so
  the last step is confirming it rather than typing or browsing to it.
- See that the sidebar accepts folders: an empty sidebar says so next to its
  Add project button, and the list outlines itself while a folder is over it.
- Drop something that cannot become a project, such as a file, and get told
  why instead of nothing happening.

## Why

Adding the first project is the one thing every new install has to do, and
until now it took a command palette, an environment, a source, and a typed
path. Dragging the folder in is how every other app on the machine takes a
directory, and it is what people try first: the empty sidebar looks like a drop
target whether or not it is one.

Dropping a folder is also the only add-project path that needs no knowledge of
the app's vocabulary, which matters most exactly when someone has just
installed it and has nothing to compare against.

## Upstream considerations

Nothing here is fork-specific and it touches upstream files on every surface it
needs (the bridge contract, the desktop preload, the sidebar, the command
palette), so it belongs upstream as a feature rather than something to carry.
Submit it and delete this entry once it merges.

While it is carried, the sidebar and command palette edits are the parts a
sync will notice, since both files move often upstream. The rest is additive:
one optional bridge method and one self-contained drop helper.

Mobile is deliberately untouched: the platform has no file manager to drag from.
Browser clients are untouched for a harder reason, that the web platform never
exposes a dropped folder's path, so only the desktop shell can resolve one.
