# 0031: Harness telemetry says T3 Code launched it

- PR: [TrogonStack/t3code#97](https://github.com/TrogonStack/t3code/pull/97)
- Status: submitted

## What you can do now

- Filter the metrics, traces, and logs that Claude Code, Codex, or any other
  harness exports to the sessions T3 Code drove, with
  `t3code.launched_by=t3code`.
- Keep your own `OTEL_RESOURCE_ATTRIBUTES`, inherited or set on a provider
  instance; the attribute is added after them, never in their place.
- Set `t3code.launched_by` yourself to tell one installation's harnesses from
  another's.

## Why

T3 Code's own telemetry names the provider and model family of each turn, but
not the exact model, tokens, or cost. The harnesses report those, and nothing
in their telemetry separated a session T3 Code launched from one started in a
terminal, so any usage report built from it described the machine, not the
product.

The key follows [0026](./0026-telemetry-says-which-app-sent-it.md) and sits
under the `t3code.` prefix, since no semantic convention says which program
launched a process.

## Upstream considerations

A plausible upstream submission. It adds one resource attribute to processes
T3 Code already starts and changes nothing for a harness that does not export
telemetry. Integrated terminals use the configured variables as overrides on
top of the inherited environment, so they skip the attribute rather than
replace the user's `OTEL_RESOURCE_ATTRIBUTES`.
