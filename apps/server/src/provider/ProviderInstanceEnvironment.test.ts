import { describe, expect, it } from "vite-plus/test";

import { mergeProviderInstanceEnvironment } from "./ProviderInstanceEnvironment.ts";

describe("mergeProviderInstanceEnvironment", () => {
  it("overrides inherited environment values and preserves empty strings", () => {
    expect(
      mergeProviderInstanceEnvironment(
        {
          variables: [
            { name: "OPENROUTER_API_KEY", value: "sk-or-test", sensitive: true },
            { name: "ANTHROPIC_API_KEY", value: "", sensitive: false },
          ],
          unresolved: [],
        },
        { ANTHROPIC_API_KEY: "inherited", PATH: "/bin" },
      ),
    ).toMatchObject({
      OPENROUTER_API_KEY: "sk-or-test",
      ANTHROPIC_API_KEY: "",
      PATH: "/bin",
    });
  });

  // The instance says this credential comes from the vault. If the vault could
  // not answer, the honest child environment has no credential at all: falling
  // back to the one the server was started with runs the agent as a different
  // account than the one the user configured, and reports it as authenticated.
  it("unsets a variable the secret store could not answer for", () => {
    const merged = mergeProviderInstanceEnvironment(
      {
        variables: [
          {
            name: "CLAUDE_SECURESTORAGE_CONFIG_DIR",
            value: "/home/u/.claude/work",
            sensitive: false,
          },
        ],
        unresolved: ["ANTHROPIC_API_KEY"],
      },
      { ANTHROPIC_API_KEY: "inherited", PATH: "/bin" },
    );

    expect("ANTHROPIC_API_KEY" in merged).toBe(false);
    expect(merged).toMatchObject({
      CLAUDE_SECURESTORAGE_CONFIG_DIR: "/home/u/.claude/work",
      PATH: "/bin",
    });
  });
});
