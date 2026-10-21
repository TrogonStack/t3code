import * as NodeChildProcess from "node:child_process";
import * as NodeFS from "node:fs";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";

import { decideTakeover, parseRunnerRecord } from "./dev-runner-guards.mjs";

const stopGracePeriodMs = 4_000;
const stopPollIntervalMs = 100;
const maxClaimAttempts = 5;

// oxlint-disable-next-line t3code/no-global-process-runtime -- Standalone dev script has no Effect runtime.
const hostPlatform = NodeOS.platform();

function runnerRecordPath(desktopDir) {
  return NodePath.join(desktopDir, ".electron-runtime", "runner.json");
}

function readRunnerRecordText(desktopDir) {
  try {
    return NodeFS.readFileSync(runnerRecordPath(desktopDir), "utf8");
  } catch {
    return null;
  }
}

function readRunnerRecord(desktopDir) {
  const text = readRunnerRecordText(desktopDir);
  return text === null ? null : parseRunnerRecord(text);
}

function readProcessCwd(pid) {
  if (hostPlatform === "linux") {
    try {
      return NodeFS.readlinkSync(`/proc/${pid}/cwd`);
    } catch {
      return null;
    }
  }
  const result = NodeChildProcess.spawnSync("lsof", ["-a", "-p", String(pid), "-d", "cwd", "-Fn"], {
    encoding: "utf8",
  });
  const line = result.stdout?.split("\n").find((entry) => entry.startsWith("n"));
  return line ? line.slice(1) : null;
}

function inspectProcess(pid) {
  const result = NodeChildProcess.spawnSync("ps", ["-p", String(pid), "-o", "command="], {
    encoding: "utf8",
  });
  const command = result.stdout?.trim();
  if (result.status !== 0 || !command) {
    return null;
  }
  return { command, cwd: readProcessCwd(pid) };
}

function isAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}

async function waitForExit(pid, timeoutMs) {
  for (let waited = 0; waited < timeoutMs; waited += stopPollIntervalMs) {
    if (!isAlive(pid)) {
      return true;
    }
    await new Promise((resolve) => setTimeout(resolve, stopPollIntervalMs));
  }
  return !isAlive(pid);
}

function removeRunnerRecordIfUnchanged(desktopDir, text) {
  if (readRunnerRecordText(desktopDir) === text) {
    NodeFS.rmSync(runnerRecordPath(desktopDir), { force: true });
  }
}

// The runner can exit on its own between inspection and each signal.
function signalIfAlive(pid, signal) {
  try {
    process.kill(pid, signal);
  } catch (error) {
    if (error?.code !== "ESRCH") {
      throw error;
    }
  }
}

async function terminate(pid) {
  signalIfAlive(pid, "SIGTERM");
  if (!(await waitForExit(pid, stopGracePeriodMs))) {
    signalIfAlive(pid, "SIGKILL");
    return waitForExit(pid, stopGracePeriodMs);
  }
  return true;
}

async function clearRecordedRunner(desktopDir, selfPid) {
  const text = readRunnerRecordText(desktopDir);
  if (text === null) {
    return { action: "none" };
  }
  const record = parseRunnerRecord(text);
  const decision = decideTakeover({
    record,
    selfPid,
    processInfo: record === null ? null : inspectProcess(record.pid),
    desktopDir,
  });
  if (decision.action === "unverifiable") {
    return decision;
  }
  if (decision.action === "stop" && !(await terminate(decision.pid))) {
    return { action: "survived", pid: decision.pid };
  }
  removeRunnerRecordIfUnchanged(desktopDir, text);
  return decision;
}

export async function stopRecordedRunner(desktopDir) {
  if (hostPlatform === "win32") {
    return { action: "unsupported" };
  }
  return clearRecordedRunner(desktopDir, 0);
}

// The record is published with link() so it appears atomically with its full contents and two
// runners starting together cannot both believe they own the checkout.
export async function acquireRunnerRecord(desktopDir, pid = process.pid) {
  const recordPath = runnerRecordPath(desktopDir);
  const contents = `${JSON.stringify({ pid })}\n`;
  NodeFS.mkdirSync(NodePath.dirname(recordPath), { recursive: true });
  if (hostPlatform === "win32") {
    NodeFS.writeFileSync(recordPath, contents);
    return { claimed: true, stoppedPids: [] };
  }

  const stagingPath = `${recordPath}.${pid}.tmp`;
  NodeFS.writeFileSync(stagingPath, contents);
  const stoppedPids = [];
  try {
    for (let attempt = 0; attempt < maxClaimAttempts; attempt += 1) {
      try {
        NodeFS.linkSync(stagingPath, recordPath);
        return { claimed: true, stoppedPids };
      } catch (error) {
        if (error?.code !== "EEXIST") {
          throw error;
        }
      }
      const decision = await clearRecordedRunner(desktopDir, pid);
      if (decision.action === "unverifiable" || decision.action === "survived") {
        return { claimed: false, blocker: decision };
      }
      if (decision.action === "stop") {
        stoppedPids.push(decision.pid);
      }
    }
    throw new Error(`another runner keeps claiming ${recordPath}`);
  } finally {
    NodeFS.rmSync(stagingPath, { force: true });
  }
}

export function releaseRunnerRecord(desktopDir, pid = process.pid) {
  if (readRunnerRecord(desktopDir)?.pid === pid) {
    NodeFS.rmSync(runnerRecordPath(desktopDir), { force: true });
  }
}
