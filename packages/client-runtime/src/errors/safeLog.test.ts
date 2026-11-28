import { describe, expect, it } from "vite-plus/test";

import { safeErrorLogAttributes } from "./safeLog.ts";

describe("safeErrorLogAttributes", () => {
  it("keeps correlation and stack frames without serializing messages or nested causes", () => {
    const cause = Object.assign(new Error("nested-cause-secret-sentinel"), {
      traceId: "trace-safe-123",
    });
    const error = Object.assign(new Error("outer-error-secret-sentinel", { cause }), {
      _tag: "ProjectRemovalError",
    });
    error.stack = [
      "ProjectRemovalError: outer-error-secret-sentinel",
      "    at removeProject (https://user:password@example.com/project.ts?token=secret#fragment)",
      "    at loadSettings (http://localhost:5173/src/useSettings.ts?t=1730700000000:162:7)",
      "removeAll@https://example.com/app.js?token=secret:12:5",
    ].join("\n");

    const attributes = safeErrorLogAttributes(error);

    expect(attributes).toMatchObject({
      errorType: "error",
      errorName: "Error",
      errorTag: "ProjectRemovalError",
      traceId: "trace-safe-123",
      stack: [
        "    at removeProject (https://example.com/project.ts)",
        "    at loadSettings (http://localhost:5173/src/useSettings.ts:162:7)",
        "removeAll@https://example.com/app.js:12:5",
      ].join("\n"),
    });
    const diagnosticText = Object.values(attributes).map(String).join("\n");
    expect(diagnosticText).not.toContain("outer-error-secret-sentinel");
    expect(diagnosticText).not.toContain("nested-cause-secret-sentinel");
    expect(diagnosticText).not.toContain("user:password");
    expect(diagnosticText).not.toContain("token=secret");
  });

  it("does not trust arbitrary object messages or tags", () => {
    const attributes = safeErrorLogAttributes({
      _tag: "payload-secret-sentinel",
      message: "message-secret-sentinel",
      cause: { traceId: "trace id with unsafe whitespace" },
    });

    expect(attributes).toEqual({ errorType: "object" });
  });

  it("keeps validated signal, exit code, reason, and operation fields", () => {
    const error = Object.assign(new Error("message-secret-sentinel"), {
      _tag: "GitCommandError",
      signal: "SIGABRT",
      exitCode: 134,
      reason: "not_a_repository",
      operation: "GitWorkflowService.listRefs",
    });

    expect(safeErrorLogAttributes(error)).toMatchObject({
      errorTag: "GitCommandError",
      signal: "SIGABRT",
      exitCode: 134,
      reason: "not_a_repository",
      operation: "GitWorkflowService.listRefs",
    });
    expect(
      safeErrorLogAttributes({ signal: "SIGKILL", exitCode: 1, operation: "Git.status" }),
    ).toEqual({ errorType: "object", signal: "SIGKILL", exitCode: 1, operation: "Git.status" });
  });

  it("drops diagnostic fields that do not match their fixed shape", () => {
    const attributes = safeErrorLogAttributes(
      Object.assign(new Error("x"), {
        signal: "SIGABRT at /Users/me/secret",
        exitCode: 1.5,
        reason: "Not A Reason: token=secret",
        operation: "has spaces /Users/me",
      }),
    );

    expect(attributes).not.toHaveProperty("signal");
    expect(attributes).not.toHaveProperty("exitCode");
    expect(attributes).not.toHaveProperty("reason");
    expect(attributes).not.toHaveProperty("operation");
    expect(safeErrorLogAttributes({ signal: "sigabrt", exitCode: "1", reason: 5 })).toEqual({
      errorType: "object",
    });
    expect(safeErrorLogAttributes({ exitCode: Number.NaN })).toEqual({ errorType: "object" });
  });
});
