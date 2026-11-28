import { describe, expect, it } from "vite-plus/test";

import { findProcessSignal } from "./processSignal.ts";

const killed = new Error("Process interrupted due to receipt of signal: 'SIGABRT'");

describe("findProcessSignal", () => {
  it("finds the signal at the end of a nested cause chain", () => {
    const platform = new Error("ChildProcess.exitCode", { cause: killed });
    expect(findProcessSignal({ message: "wrapper", cause: platform })).toBe("SIGABRT");
  });

  it("reads a signal a wrapper already recorded", () => {
    expect(findProcessSignal({ signal: "SIGKILL", message: "wrapper" })).toBe("SIGKILL");
  });

  it("ignores unrelated causes, malformed signals, and cycles", () => {
    const cyclic: { message: string; cause?: unknown } = { message: "loop" };
    cyclic.cause = cyclic;
    expect(findProcessSignal(cyclic)).toBeUndefined();
    expect(findProcessSignal({ signal: "not a signal" })).toBeUndefined();
    expect(findProcessSignal(new Error("exited with signal: 'sigabrt'"))).toBeUndefined();
    expect(findProcessSignal(undefined)).toBeUndefined();
  });
});
