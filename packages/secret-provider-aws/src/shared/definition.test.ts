import * as Schema from "effect/Schema";
import { describe, expect, it } from "@effect/vitest";

import { AwsSecretsManagerSecretProviderConfig, definition } from "./definition.ts";

describe("AWS Secrets Manager definition", () => {
  it("accepts a name or ARN with an optional JSON key", () => {
    for (const reference of [
      "prod/openai",
      "prod/openai#api_key",
      "arn:aws:secretsmanager:us-east-1:000000000000:secret:prod",
    ]) {
      expect(definition.isReference(reference), reference).toBe(true);
    }
  });

  it("rejects an empty id, an empty key, and a flag-shaped id", () => {
    for (const reference of ["", "#key", "prod#", "--help", " prod", "prod\n"]) {
      expect(definition.isReference(reference), reference).toBe(false);
    }
  });

  it("validates region and profile", () => {
    const decode = Schema.decodeUnknownExit(AwsSecretsManagerSecretProviderConfig);
    expect(decode({ region: "us-east-1", profile: "work" })._tag).toBe("Success");
    expect(decode({ region: "nowhere" })._tag).toBe("Failure");
    expect(decode({ region: "us-east-1", profile: "-x" })._tag).toBe("Failure");
  });
});
