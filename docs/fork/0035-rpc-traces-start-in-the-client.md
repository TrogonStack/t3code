# 0035: A server call's trace starts where the client made it

- PR: [TrogonStack/t3code#67](https://github.com/TrogonStack/t3code/pull/67)
- Status: active

## What you can do now

- Follow one action from the client into the server in a single trace. The span the server opens
  for a websocket call is a child of the span the client sent with it, so a slow click and the
  server work behind it show up together.
- Still find the connection a call arrived on. Each call's span keeps a link to the socket's
  connection span instead of being parented to it.

## Why

Upstream starts every websocket call in a trace of its own, linked to the connection, so a
long-lived socket no longer grows one trace past the collector's per-trace limit. The fork already
avoided that limit for every client that sends a span, which the web and mobile clients do, and
also keeps the client's half of the story. Taking upstream's version would cut every client trace
off at the socket.

## Upstream considerations

A good candidate to propose as a refinement of upstream's fix: join the client's trace when one
was sent and start a new root only when none was. A client that sends no span still parents its
calls to the connection span here, which is the case upstream's fix covers and this one does not.

The rebase burden is small: the request span options are handed to the RPC server, and the
instrumentation middleware only annotates that span. A sync that touches how RPC spans open needs
to keep that split.
