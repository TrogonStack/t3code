import { CustomModelSetting, makeProviderSettingsSchema, TrimmedString } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

export const CursorSettings = makeProviderSettingsSchema(
  {
    enabled: Schema.Boolean.pipe(
      Schema.withDecodingDefault(Effect.succeed(false)),
      Schema.annotateKey({ providerSettingsForm: { hidden: true } }),
    ),
    // Preserve V1 CLI fields when V2's SDK rewrites shared settings.
    binaryPath: Schema.optionalKey(TrimmedString).pipe(
      Schema.annotateKey({ providerSettingsForm: { hidden: true } }),
    ),
    apiEndpoint: Schema.optionalKey(TrimmedString).pipe(
      Schema.annotateKey({ providerSettingsForm: { hidden: true } }),
    ),
    globalConfig: Schema.Boolean.pipe(
      Schema.withDecodingDefault(Effect.succeed(true)),
      Schema.annotateKey({
        title: "Use global Cursor config",
        description:
          "Load MCP servers, hooks, and sandbox settings from ~/.cursor, and honor the commit and PR attribution choices in its cli-config.json.",
      }),
    ),
    instructionsPath: Schema.optionalKey(TrimmedString).pipe(
      Schema.annotateKey({
        title: "Global instructions file",
        description:
          "Markdown file sent to Cursor at the start of every new thread, such as your global AGENTS.md. A leading ~/ means your home directory.",
        providerSettingsForm: { placeholder: "~/AGENTS.md", clearWhenEmpty: "omit" },
      }),
    ),
    customModels: Schema.Array(CustomModelSetting).pipe(
      Schema.withDecodingDefault(Effect.succeed([])),
      Schema.annotateKey({ providerSettingsForm: { hidden: true } }),
    ),
  },
  {
    order: ["globalConfig", "instructionsPath"],
  },
);
export type CursorSettings = typeof CursorSettings.Type;
