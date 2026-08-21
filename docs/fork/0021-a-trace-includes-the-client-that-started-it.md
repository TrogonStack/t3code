# 0021: A trace includes the client that started it

- PR: [TrogonStack/t3code#34](https://github.com/TrogonStack/t3code/pull/34)
- Status: active

## What you can do now

- Read a trace from the click that started it. Work you begin in the app now
  appears in the same trace as the server work it caused, so the path from an
  action to its result is one story instead of a fragment of one.
- Tell a slow app from a slow machine. Time the client spends before a request
  ever leaves is now visible next to the time the server spent on it, which is
  the first question worth asking when something feels slow and the server
  looks fine.
- Stop chasing parents that do not exist. Server work used to point at a client
  step that never arrived anywhere, so traces read as though something had gone
  missing in transit. Nothing was lost in transit; the client step simply never
  left.
- Get it on the desktop app too, on the same trace destination you already
  configured. There is nothing new to turn on.

## Why

The app has produced its own trace steps for a long time, and it has always
told the server which step to attach its work to. What it never did was send
those steps anywhere. The result was the most expensive kind of wrong: traces
that looked complete, arrived on time, and described only half of what happened,
with a root that pointed at something no backend had ever been given.

That gap is worst exactly where tracing earns its keep. Turn latency is a
question about the whole path, and a trace that starts at the server can only
ever answer the second half of it. Anyone reading one had to know, from
outside the tool, that the missing half was missing rather than empty.

This was not a decision anyone made. Client tracing was built, wired up, and
then quietly unwired by a later change to how the app builds itself, and the
part that went missing was the only part that had no test. It has been off ever
since, without a warning, a log line, or an empty panel to hint at it.

## Upstream considerations

This belongs upstream and should be easy for them to take: it restores behavior
upstream built and intended, changes no API, and adds nothing that is specific
to this fork. The divergence disappears the moment upstream reconnects it, and
this entry goes with it.

The rebase burden is small but has a sharp edge. The trace destination has to be
attached where the app assembles its shared services, and a sync that rewrites
that assembly has to bring the attachment along. That is precisely how it was
lost the first time, so it now fails a test when it goes missing instead of
failing silently.
