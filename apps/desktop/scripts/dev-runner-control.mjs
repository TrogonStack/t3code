import * as NodeChildProcess from "node:child_process";
import * as NodeFS from "node:fs";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";

import { decideTakeover, parseRunnerRecord } from "./dev-runner-guards.mjs";

const stopGracePeriodMs = 4_000;
const stopPollIntervalMs = 100;

// oxlint-disable-next-line t3code/no-global-process-runtime -- Standalone dev script has no Effect runtime.
const hostPlatform = NodeOS.platform();

export function runnerRecordPath(desktopDir) {
  return NodePath.join(desktopDir, ".electron-runtime", "runner.json");
}

function readRunnerRecord(desktopDir) {
  try {
    return parseRunnerRecord(NodeFS.readFileSync(runnerRecordPath(desktopDir), "utf8"));
  } catch {
    return null;
  }
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

export async function stopPreviousRunner(desktopDir, selfPid = process.pid) {
  if (hostPlatform === "win32") {
    return { stoppedPid: null };
  }

  const record = readRunnerRecord(desktopDir);
  const decision = decideTakeover({
    record,
    selfPid,
    processInfo: record === null ? null : inspectProcess(record.pid),
    desktopDir,
  });
  if (decision.action !== "stop") {
    return { stoppedPid: null, refusedPid: decision.action === "refuse" ? decision.pid : null };
  }

  process.kill(decision.pid, "SIGTERM");
  if (!(await waitForExit(decision.pid, stopGracePeriodMs))) {
    process.kill(decision.pid, "SIGKILL");
    await waitForExit(decision.pid, stopGracePeriodMs);
  }
  return { stoppedPid: decision.pid };
}

export function claimRunnerRecord(desktopDir, pid = process.pid) {
  NodeFS.mkdirSync(NodePath.dirname(runnerRecordPath(desktopDir)), { recursive: true });
  NodeFS.writeFileSync(runnerRecordPath(desktopDir), `${JSON.stringify({ pid })}\n`);
}

export function releaseRunnerRecord(desktopDir, pid = process.pid) {
  if (readRunnerRecord(desktopDir)?.pid === pid) {
    NodeFS.rmSync(runnerRecordPath(desktopDir), { force: true });
  }
}
