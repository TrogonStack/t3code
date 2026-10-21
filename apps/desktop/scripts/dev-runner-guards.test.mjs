import { assert, describe, it } from "vite-plus/test";

import {
  createCrashLoopBreaker,
  decideTakeover,
  isVerifiedRunner,
  parseRunnerRecord,
} from "./dev-runner-guards.mjs";

const desktopDir = "/work/a/apps/desktop";
const runner = { command: "node scripts/dev-electron.mjs", cwd: desktopDir };

describe("runner record", () => {
  it("accepts a pid and rejects malformed or unsafe records", () => {
    assert.deepEqual(parseRunnerRecord('{"pid": 4242}'), { pid: 4242 });
    for (const text of ["", "not json", "{}", '{"pid":"7"}', '{"pid":1}', '{"pid":-3}']) {
      assert.isNull(parseRunnerRecord(text));
    }
  });
});

describe("takeover decision", () => {
  const decide = (overrides) =>
    decideTakeover({
      record: { pid: 100 },
      selfPid: 200,
      processInfo: runner,
      desktopDir,
      ...overrides,
    });

  it("stops a live runner from the same checkout", () => {
    assert.deepEqual(decide({}), { action: "stop", pid: 100 });
  });

  it("does nothing without a record, for a dead pid, or for itself", () => {
    assert.deepEqual(decide({ record: null }), { action: "none" });
    assert.deepEqual(decide({ processInfo: null }), { action: "none" });
    assert.deepEqual(decide({ selfPid: 100 }), { action: "none" });
  });

  it("treats a pid reused by another process as a stale record", () => {
    assert.deepEqual(decide({ processInfo: { command: "sleep 30", cwd: desktopDir } }), {
      action: "stale",
      pid: 100,
    });
    assert.deepEqual(decide({ processInfo: { ...runner, cwd: "/work/b/apps/desktop" } }), {
      action: "stale",
      pid: 100,
    });
  });

  it("will not stop or replace a runner whose working directory is unknown", () => {
    assert.deepEqual(decide({ processInfo: { ...runner, cwd: null } }), {
      action: "unverifiable",
      pid: 100,
    });
    assert.isFalse(isVerifiedRunner({ ...runner, cwd: null }, desktopDir));
  });

  it("matches the runner by its node entry script, not by a substring", () => {
    const at = (command) => isVerifiedRunner({ command, cwd: desktopDir }, desktopDir);
    assert.isTrue(at("/usr/local/bin/node --enable-source-maps scripts/dev-electron.mjs"));
    assert.isFalse(at("tail -f scripts/dev-electron.mjs"));
    assert.isFalse(at("node scripts/dev-electron.mjs.bak"));
  });
});

describe("crash loop breaker", () => {
  it("trips on three abnormal exits inside the window", () => {
    const breaker = createCrashLoopBreaker();
    assert.isFalse(breaker.recordAbnormalExit(0).tripped);
    assert.isFalse(breaker.recordAbnormalExit(4_000).tripped);
    assert.isTrue(breaker.recordAbnormalExit(9_000).tripped);
  });

  it("forgets exits that fall outside the window", () => {
    const breaker = createCrashLoopBreaker();
    breaker.recordAbnormalExit(0);
    breaker.recordAbnormalExit(5_000);
    assert.isFalse(breaker.recordAbnormalExit(10_000).tripped);
    assert.isFalse(breaker.recordAbnormalExit(30_000).tripped);
  });

  it("starts counting again after a rearm", () => {
    const breaker = createCrashLoopBreaker();
    breaker.recordAbnormalExit(0);
    breaker.recordAbnormalExit(1_000);
    breaker.rearm();
    assert.isFalse(breaker.recordAbnormalExit(2_000).tripped);
    assert.isFalse(breaker.recordAbnormalExit(3_000).tripped);
    assert.isTrue(breaker.recordAbnormalExit(4_000).tripped);
  });
});
