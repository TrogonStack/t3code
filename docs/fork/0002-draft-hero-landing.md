# 0002: Draft hero landing on the index route

- PR: [TrogonStack/t3code#2](https://github.com/TrogonStack/t3code/pull/2)
- Status: active

## What you can do now

- Opening the app (or landing anywhere without an active thread) drops you
  straight into a ready-to-send composer for your most recently active
  project, under a centered "What should we do in {project}?" headline. Type
  and send; the thread is created as usual.
- The project name in the headline is a dropdown: pick another project and
  the screen swaps to that project's draft, keeping each project's composer
  text, model selection, and branch settings intact.
- The hero stays visually stable while the app connects: the branch toolbar
  and connection banners appear below the composer without shifting the
  headline or composer.
- With no projects yet, the screen offers a single Add project action that
  opens the existing add-project flow.

## Why

The index route used to be a dead end that told you to go click something
else. The first screen should be a prompt, so launching the app and typing is
the whole flow, with zero clicks in between.

## Upstream considerations

Built entirely on the existing draft-thread machinery and public hooks, so
the rebase burden is low. This is a product-opinion change to the landing
experience; upstream may prefer their neutral empty state, so it may stay a
fork divergence permanently.
