# 0042: MCP apps with nothing to show leave no empty box

- PR: [TrogonStack/t3code#235](https://github.com/TrogonStack/t3code/pull/235)
- Status: active

## What you can do now

- Scroll a thread without an empty box under every call of a tool whose MCP app had nothing to
  render. An app that reports no height collapses, on web and mobile, and still grows if content
  arrives later.
- Use MCP apps that bundle analytics SDKs without them failing on cookie access inside the
  sandbox.

## Why

Some MCP apps attach to every call of a general-purpose tool and report a height of 0 when a
result has nothing to show. Flooring that to a minimum height left a blank box per call, which
buried the thread in empty frames.

## Upstream considerations

A good candidate to submit as a bug fix. The rebase burden sits in the MCP app frame's height
handling on web and mobile, which upstream reworks now and then.
