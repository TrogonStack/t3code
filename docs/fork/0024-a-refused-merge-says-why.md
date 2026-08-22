# 0024: A refused merge says why, and an administrator can merge anyway

- PR: [TrogonStack/t3code#38](https://github.com/TrogonStack/t3code/pull/38)
- Status: active

## What you can do now

- Read the reason a merge was refused. A pull request the base branch's rules
  hold back, a branch that conflicts, and a pull request somebody already
  merged now each say so in the toast, instead of all three arriving as the
  same "check that you have write access" guess.
- Merge past the branch's own rules where GitHub lets you. Administrators of a
  repository get a "Squash past branch rules" entry in the pull request's
  action menu, behind its own confirmation, which does what
  `gh pr merge --admin` does.
- Take the override from where the refusal happened. When a merge is held back
  by the base branch's rules and the signed-in account may stand them down, the
  toast is replaced by a confirmation that leads with the host's own sentence
  and offers the same merge again past those rules. A refusal nobody can
  override, and a bypass that was itself refused, still arrive as a toast.
- Keep the offer out of everyone else's way. The entry appears only where the
  host supports the override and the signed-in account administers the
  repository, and never on a draft or a conflicting branch, since neither of
  those is a rule that can be stood down.

## Why

A pull request waiting on a review it will never get is a normal end to a day's
work, and the app answered it with a hint that pointed at the wrong thing.
Write access was fine, the checks were fine, and nothing conflicted: the base
branch simply required an approval. The reason was sitting in the host's own
reply and was being thrown away one layer below the toast, which left the
reader pressing the same button again.

The override is the other half of the same moment. Somebody who administers the
repository, working alone on their own project, has the authority to merge and
had to leave the app for a terminal to use it. Confirming it separately and
wording it as what stops being enforced keeps that from being a button anyone
presses by accident.

Explaining the refusal without offering the way out is still a dead end. The
reader is told no, and the one thing that would get them past it lives in a
menu they have no reason to open at that point. So the reason travels as a kind
and not only as prose, which is what lets the page tell the refusal an
administrator can stand down from the ones nobody can, and only then ask.

## Upstream considerations

Both halves are good upstream candidates and are worth submitting together, as
the second is hard to justify without the first.

The reason vocabulary follows the shape upstream already uses: no tool output
crosses the process boundary, and each recognized reason gets a sentence
written in the contract. The bypass is optional in the contract and reported
per provider, so GitLab, Bitbucket and Azure DevOps carry on saying nothing and
offering nothing.

The rebase burden is small but spread across the pull request stack, from the
process boundary to the detail panel. If upstream takes it, the divergence goes
and this entry with it.
