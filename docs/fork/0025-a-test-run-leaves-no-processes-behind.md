# 0025: A test run leaves no processes behind

- PR: [TrogonStack/t3code#57](https://github.com/TrogonStack/t3code/pull/57)
- Status: active

## What you can do now

- Run the suite as often as you like without the machine accumulating
  stranded processes. What the tests start, the tests take with them when
  they go.
- Trust your process list again. A stray provider process in it now means
  something is genuinely running, not that you ran the tests last Tuesday.
- Leave a long-lived machine running the suite on a loop without it slowly
  filling with residue that only a reboot clears.

## Why

The leak was silent in the way that matters: nothing failed, nothing was
logged, and every run reported green. The cost accrued outside the test
report entirely, one process per run, each holding its memory and its file
handles for as long as the machine stayed up. Found in the wild, the oldest
survivors were days old and their temporary directories had long since been
deleted out from under them.

Silence is what makes it worth fixing rather than living with. A test that
fails gets attention on the spot. A test that quietly leaves something
behind gets attention weeks later, from whoever is wondering why a
workstation is sluggish, and by then the connection back to the suite is
gone. The processes are idle, so nothing in the usual places points at them.

It also erodes a thing the suite is supposed to be good for. Some of these
tests exist to check that provider processes are started and cleaned up
correctly, and a harness that strands its own children is poorly placed to
make claims about cleanup.

## Upstream considerations

A clean upstream submission. This carries no fork-specific intent, fixes
upstream's own tests, and changes no product behavior, so there is nothing
here upstream would want to weigh. Once an equivalent lands there, this
entry goes.

Worth flagging for a sync rather than a rebase: the divergence lives
entirely in test fixtures, so a sync that takes upstream's copy of either
file reintroduces the leak without anything going red. The suite passes
either way, and the symptom shows up only in the process list on whatever
machine ran it.
