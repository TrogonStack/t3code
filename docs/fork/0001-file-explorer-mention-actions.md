# 0001: File explorer mention actions and zoom-aware context menus

- PR: [TrogonStack/t3code#1](https://github.com/TrogonStack/t3code/pull/1)
- Status: active

## What this adds

Right-clicking a file or directory in the Files panel opens a context menu
with two actions:

- **Copy mention** copies the composer mention form, `[basename](path)`, so it
  can be pasted anywhere in a prompt and render as a file pill.
- **Add to chat** appends the mention directly to the active composer.

Two supporting changes were required to make that work end to end:

- The composer only tokenized mentions inserted programmatically, so pasted
  mention text stayed plain text. A paste plugin in `ComposerPromptEditor` now
  parses pasted text with the shared inline-token grammar and inserts pill
  nodes. This benefits any pasted mention, not just ones copied from the file
  explorer.
- Native context menus positioned by coordinates drifted whenever the window
  was zoomed (Cmd+= / Cmd+-), proportionally to the distance from the window
  origin. `ElectronMenu` now converts renderer CSS pixels to window points
  using the web contents zoom factor. This fixes every coordinate-positioned
  context menu in the desktop app, not only the new one.

## Touched files

- `apps/web/src/components/files/FileBrowserPanel.tsx`
- `apps/web/src/components/ComposerPromptEditor.tsx`
- `apps/desktop/src/electron/ElectronMenu.ts`

## Upstream considerations

The zoom fix in `ElectronMenu.ts` is a genuine bug fix that stands alone and
is the strongest candidate to submit upstream once contributions open
(upstream is not accepting contributions at the time of writing). The mention
actions are a feature upstream may or may not want; they depend only on
public composer and tree APIs, so the rebase burden is low.
