# 0037: Generated files start folded in a pull request's diff

- PR: [TrogonStack/t3code#219](https://github.com/TrogonStack/t3code/pull/219)
- Status: active

## What you can do now

- Review a pull request without scrolling past lockfiles, generated route trees, and other files
  the repository's `.gitattributes` marks `linguist-generated`. They start folded in the Code tab,
  the way GitHub shows them.
- Turn that off in settings if you want every file open.

## Why

Generated files are often the largest part of a diff and the part nobody reviews line by line.
Showing them open buries the changes that need a reader.

## Upstream considerations

A reasonable candidate to submit: it follows a convention GitHub already uses and is behind a
setting. The rebase burden sits in the diff read and the Code tab's fold rule, both of which
upstream changes often, so a sync touching either needs to carry the generated paths forward.
