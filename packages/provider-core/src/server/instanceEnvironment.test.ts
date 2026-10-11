import * as NodeServices from "@effect/platform-node/NodeServices";
import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Path from "effect/Path";
import * as HostProcess from "@t3tools/shared/HostProcess";

import { ProviderInstanceEnvironment } from "@t3tools/contracts";
import * as Schema from "effect/Schema";

import {
  literalProviderInstanceEnvironment,
  mergeProviderInstanceEnvironment,
  overlayProviderInstanceEnvironment,
} from "./instanceEnvironment.ts";

const LAUNCHED = "t3code.launched=true";

describe("mergeProviderInstanceEnvironment", () => {
  it.effect.each([
    { value: "~/.account", tail: ".account" },
    { value: "~\\.account\\work", tail: ".account\\work" },
  ])("expands configured provider homes set to $value", ({ value, tail }) =>
    Effect.gen(function* () {
      const path = yield* Path.Path;
      const baseEnv = {
        CODEX_HOME: "~/.inherited-codex",
        CLAUDE_CONFIG_DIR: "~/.inherited-claude",
      };
      const environment = yield* mergeProviderInstanceEnvironment(
        [
          { name: "CODEX_HOME", value, sensitive: false },
          { name: "CLAUDE_CONFIG_DIR", value, sensitive: false },
          { name: "CUSTOM_VALUE", value, sensitive: false },
        ],
        baseEnv,
      );

      expect(environment).toEqual({
        CODEX_HOME: path.join("/home/ada", tail),
        CLAUDE_CONFIG_DIR: path.join("/home/ada", tail),
        CUSTOM_VALUE: value,
        OTEL_RESOURCE_ATTRIBUTES: LAUNCHED,
      });
      expect(baseEnv).toEqual({
        CODEX_HOME: "~/.inherited-codex",
        CLAUDE_CONFIG_DIR: "~/.inherited-claude",
      });
    }).pipe(
      Effect.provideService(HostProcess.HomeDirectory, "/home/ada"),
      Effect.provide(NodeServices.layer),
    ),
  );

  it.effect("leaves inherited provider homes unchanged", () =>
    Effect.gen(function* () {
      const baseEnv = { CODEX_HOME: "~/.codex", CLAUDE_CONFIG_DIR: "~\\.claude" };

      expect(
        yield* mergeProviderInstanceEnvironment(
          [{ name: "CUSTOM_VALUE", value: "~/.custom", sensitive: false }],
          baseEnv,
        ),
      ).toEqual({ ...baseEnv, CUSTOM_VALUE: "~/.custom", OTEL_RESOURCE_ATTRIBUTES: LAUNCHED });
    }),
  );

  it.effect("overrides inherited environment values and preserves empty strings", () =>
    Effect.gen(function* () {
      expect(
        yield* mergeProviderInstanceEnvironment(
          [
            { name: "OPENROUTER_API_KEY", value: "sk-or-test", sensitive: true },
            { name: "ANTHROPIC_API_KEY", value: "", sensitive: false },
          ],
          { ANTHROPIC_API_KEY: "inherited", PATH: "/bin" },
        ),
      ).toMatchObject({
        OPENROUTER_API_KEY: "sk-or-test",
        ANTHROPIC_API_KEY: "",
        PATH: "/bin",
      });
    }),
  );
});

describe("launched resource attribute", () => {
  it.effect("marks a process launched without configured variables", () =>
    Effect.gen(function* () {
      const baseEnv = { PATH: "/bin" };

      expect(yield* mergeProviderInstanceEnvironment(undefined, baseEnv)).toEqual({
        PATH: "/bin",
        OTEL_RESOURCE_ATTRIBUTES: LAUNCHED,
      });
      expect(baseEnv).toEqual({ PATH: "/bin" });
    }),
  );

  it.effect("extends inherited resource attributes", () =>
    Effect.gen(function* () {
      expect(
        yield* mergeProviderInstanceEnvironment([], {
          OTEL_RESOURCE_ATTRIBUTES: "org.name=Acme,deployment.environment.name=dev",
        }),
      ).toEqual({
        OTEL_RESOURCE_ATTRIBUTES: `org.name=Acme,deployment.environment.name=dev,${LAUNCHED}`,
      });
    }),
  );

  it.effect("extends resource attributes a provider instance configures", () =>
    Effect.gen(function* () {
      expect(
        yield* mergeProviderInstanceEnvironment(
          [{ name: "OTEL_RESOURCE_ATTRIBUTES", value: "org.name=Acme", sensitive: false }],
          { OTEL_RESOURCE_ATTRIBUTES: "org.name=Inherited" },
        ),
      ).toEqual({ OTEL_RESOURCE_ATTRIBUTES: `org.name=Acme,${LAUNCHED}` });
    }).pipe(Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
  );

  it.effect.each([
    "t3code.launched=false",
    "org.name=Acme, t3code.launched=false",
    "t3code%2Elaunched=false",
  ])("keeps a value already set in %s", (attributes) =>
    Effect.gen(function* () {
      expect(
        yield* mergeProviderInstanceEnvironment(undefined, {
          OTEL_RESOURCE_ATTRIBUTES: attributes,
        }),
      ).toEqual({ OTEL_RESOURCE_ATTRIBUTES: attributes });
    }),
  );

  it.effect("marks a process once however often its environment is merged", () =>
    Effect.gen(function* () {
      const once = yield* mergeProviderInstanceEnvironment(undefined, { PATH: "/bin" });

      expect(yield* mergeProviderInstanceEnvironment(undefined, once)).toEqual(once);
    }),
  );

  it.effect("leaves an override-only environment unmarked", () =>
    Effect.gen(function* () {
      expect(
        yield* overlayProviderInstanceEnvironment(
          [{ name: "CUSTOM_VALUE", value: "value", sensitive: false }],
          {},
        ),
      ).toEqual({ CUSTOM_VALUE: "value" });
    }).pipe(Effect.provideService(HostProcess.HomeDirectory, "/home/ada")),
  );
});

const decodeProviderInstanceEnvironment = Schema.decodeSync(ProviderInstanceEnvironment);

describe("literalProviderInstanceEnvironment", () => {
  it("keeps every literal and leaves out secret sources", () => {
    const environment = decodeProviderInstanceEnvironment([
      { name: "CODEX_HOME", value: "~/.codex-work" },
      { name: "REFERENCE_LOOKALIKE", value: "op://Private/item/field" },
      {
        name: "TOKEN",
        value: { kind: "1password", reference: "op://Private/item/field", account: "my" },
      },
    ]);

    expect(literalProviderInstanceEnvironment(environment)).toEqual([
      { name: "CODEX_HOME", value: "~/.codex-work", sensitive: false },
      { name: "REFERENCE_LOOKALIKE", value: "op://Private/item/field", sensitive: false },
    ]);
  });
});
