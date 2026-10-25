import * as NodePath from "node:path";

const RUNNER_SCRIPT_NAME = "dev-electron.mjs";

const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

// `pkill -f` patterns for the Electron apps a runner launches from one checkout.
export function devAppPattern(desktopDir) {
  return `(^| )--t3code-dev-root=${escapeRegExp(desktopDir)}( |$)`;
}

// The backend Electron spawns carries no dev-root tag and is the runner's grandchild, so neither
// the tag nor `pkill -P` reaches it once its Electron parent is gone. A hung one keeps the T3
// home's ownership lock and every later start fails with ServerAlreadyRunningError.
export function devBackendPattern(desktopDir) {
  const serverEntry = NodePath.join(NodePath.dirname(desktopDir), "server", "dist", "bin.mjs");
  return `(^| )${escapeRegExp(serverEntry)} --bootstrap-fd( |$)`;
}

export function parseRunnerRecord(text) {
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  const pid = parsed?.pid;
  return Number.isInteger(pid) && pid > 1 ? { pid } : null;
}

function isRunnerCommand(command) {
  const [executable = "", ...args] = command.trim().split(/\s+/);
  const script = args.find((arg) => !arg.startsWith("-"));
  return (
    /^node(?:\.exe)?$/.test(executable.split(/[\\/]/).at(-1)) &&
    script !== undefined &&
    script.split(/[\\/]/).at(-1) === RUNNER_SCRIPT_NAME
  );
}

export function isVerifiedRunner(processInfo, desktopDir) {
  return (
    processInfo !== null && isRunnerCommand(processInfo.command) && processInfo.cwd === desktopDir
  );
}

// `processInfo` is null when the pid is not alive. Only a verified runner is signaled: a pid
// running something else was reused, so its record is stale, while a runner whose working
// directory cannot be read may belong to this checkout and must be neither signaled nor replaced.
export function decideTakeover({ record, selfPid, processInfo, desktopDir }) {
  if (record === null || record.pid === selfPid || processInfo === null) {
    return { action: "none" };
  }
  if (isVerifiedRunner(processInfo, desktopDir)) {
    return { action: "stop", pid: record.pid };
  }
  if (isRunnerCommand(processInfo.command) && processInfo.cwd === null) {
    return { action: "unverifiable", pid: record.pid };
  }
  return { action: "stale", pid: record.pid };
}

export function createCrashLoopBreaker({ maxAbnormalExits = 3, windowMs = 10_000 } = {}) {
  let exitTimes = [];
  return {
    recordAbnormalExit(nowMs) {
      exitTimes = [...exitTimes.filter((time) => nowMs - time < windowMs), nowMs];
      return { tripped: exitTimes.length >= maxAbnormalExits };
    },
    rearm() {
      exitTimes = [];
    },
  };
}
