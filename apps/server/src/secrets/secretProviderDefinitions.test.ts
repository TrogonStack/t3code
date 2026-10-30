import { expect, it } from "@effect/vitest";

import {
  BUILT_IN_SECRET_PROVIDER_DRIVER_LIST,
  secretProviderDefinitions,
} from "./secretProviderDefinitions.ts";

it("runs a driver for every secret provider the server and clients read, in their order", () => {
  expect(BUILT_IN_SECRET_PROVIDER_DRIVER_LIST.map((driver) => driver.definition)).toEqual(
    secretProviderDefinitions.definitions,
  );
});

it("registers the built-in kinds in a stable order", () => {
  expect(secretProviderDefinitions.definitions.map((definition) => definition.kind)).toEqual([
    "one_password",
    "openbao",
    "aws_secrets_manager",
  ]);
});
