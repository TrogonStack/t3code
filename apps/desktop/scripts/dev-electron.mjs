import * as NodeChildProcess from "node:child_process";
import * as NodeFS from "node:fs";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";

import {
  desktopDir,
  resolveDevProtocolClient,
  resolveElectronLaunchCommand,
} from "./electron-launcher.mjs";
import { acquireRunnerRecord, releaseRunnerRecord } from "./dev-runner-control.mjs";
import { createCrashLoopBreaker, devAppPattern, devBackendPattern } from "./dev-runner-guards.mjs";
import { waitForResources } from "./wait-for-resources.mjs";

const devServerUrl = process.env.VITE_DEV_SERVER_URL?.trim();
if (!devServerUrl) {
  throw new Error("VITE_DEV_SERVER_URL is required for desktop development.");
}

const devServer = new URL(devServerUrl);
const port = Number.parseInt(devServer.port, 10);
if (!Number.isInteger(port) || port <= 0) {
  throw new Error(`VITE_DEV_SERVER_URL must include an explicit port: ${devServerUrl}`);
}

const requiredFiles = [
  "dist-electron/main.cjs",
  "dist-electron/electron/WindowsForegroundFocusWorker.cjs",
  "dist-electron/preload.cjs",
  "dist-electron/snapShot/GlobalShiftShortcutWorker.cjs",
  "dist-electron/snapShot/RegionSnapShotWorker.cjs",
  "dist-electron/snapShot/SnapShotAccessibilityWorker.cjs",
  "../server/dist/bin.mjs",
];
const watchedDirectories = [
  { directory: "dist-electron", files: new Set(["main.cjs", "preload.cjs"]) },
  {
    directory: "dist-electron/electron",
    files: new Set(["WindowsForegroundFocusWorker.cjs"]),
  },
  {
    directory: "dist-electron/snapShot",
    files: new Set([
      "GlobalShiftShortcutWorker.cjs",
      "RegionSnapShotWorker.cjs",
      "SnapShotAccessibilityWorker.cjs",
    ]),
  },
  { directory: "../server/dist", files: new Set(["bin.mjs"]) },
];
const forcedShutdownTimeoutMs = 1_500;
const backendShutdownTimeoutMs = 4_000;
const backendPollIntervalMs = 100;
const restartDebounceMs = 120;
const childTreeGracePeriodMs = 1_200;
const orphanCheckIntervalMs = 1_000;
const remoteDebuggingPort = process.env.T3CODE_DESKTOP_REMOTE_DEBUGGING_PORT?.trim();
// oxlint-disable-next-line t3code/no-global-process-runtime -- Standalone dev script has no Effect runtime.
const hostPlatform = NodeOS.platform();

const initialParentPid = process.ppid;

// Two runners in one checkout rebuild the same launcher bundle concurrently and kill each
// other's Electron processes, so a new runner replaces the previous one.
const claim = await acquireRunnerRecord(desktopDir);
if (!claim.claimed) {
  const reason =
    claim.blocker.action === "survived"
      ? "did not exit after SIGKILL"
      : "may still own this checkout but its working directory could not be verified";
  console.error(
    `[desktop-dev] runner ${claim.blocker.pid} ${reason}. ` +
      "Stop it, or delete .electron-runtime/runner.json, then retry.",
  );
  process.exit(1);
}
for (const stoppedPid of claim.stoppedPids) {
  console.log(`[desktop-dev] stopped previous runner ${stoppedPid} for this checkout`);
}

NodeChildProcess.execFileSync(
  process.execPath,
  [NodePath.join(desktopDir, "scripts/build-browser-secret.mjs")],
  { stdio: "inherit" },
);

await waitForResources({
  baseDir: desktopDir,
  files: requiredFiles,
  tcpHost: devServer.hostname,
  tcpPort: port,
});

if (process.ppid !== initialParentPid) {
  releaseRunnerRecord(desktopDir);
  process.exit(0);
}

const childEnv = { ...process.env };
delete childEnv.ELECTRON_RUN_AS_NODE;
const devProtocolClient = resolveDevProtocolClient();
if (devProtocolClient) {
  childEnv.T3CODE_DESKTOP_APP_USER_MODEL_ID = devProtocolClient.appBundleId;
  childEnv.T3CODE_DESKTOP_PROTOCOL_REGISTRATION_MANAGED = "1";
}

let shuttingDown = false;
let restartTimer = null;
let currentApp = null;
let restartQueue = Promise.resolve();
const expectedExits = new WeakSet();
const watchers = [];
const failedApps = new WeakSet();
const crashLoopBreaker = createCrashLoopBreaker();

function killChildTreeByPid(pid, signal) {
  if (hostPlatform === "win32" || typeof pid !== "number") {
    return;
  }

  NodeChildProcess.spawnSync("pkill", [`-${signal}`, "-P", String(pid)], { stdio: "ignore" });
}

function cleanupStaleDevApps() {
  if (hostPlatform === "win32") {
    return;
  }

  NodeChildProcess.spawnSync("pkill", ["-f", "--", devAppPattern(desktopDir)], {
    stdio: "ignore",
  });
}

// pkill and pgrep exit 0 only when the pattern matched a process.
function matchDevBackends(command, args) {
  return (
    NodeChildProcess.spawnSync(command, [...args, "-f", "--", devBackendPattern(desktopDir)], {
      stdio: "ignore",
    }).status === 0
  );
}

async function stopStaleDevBackends() {
  if (hostPlatform === "win32" || !matchDevBackends("pkill", ["-TERM"])) {
    return;
  }

  for (let waited = 0; waited < backendShutdownTimeoutMs; waited += backendPollIntervalMs) {
    await new Promise((resolve) => setTimeout(resolve, backendPollIntervalMs));
    if (!matchDevBackends("pgrep", [])) {
      return;
    }
  }
  matchDevBackends("pkill", ["-KILL"]);
}

function startApp() {
  if (shuttingDown || currentApp !== null) {
    return;
  }

  const electronArgs = remoteDebuggingPort
    ? [`--remote-debugging-port=${remoteDebuggingPort}`]
    : [];
  const launchArgs = devProtocolClient
    ? electronArgs
    : [...electronArgs, `--t3code-dev-root=${desktopDir}`, "dist-electron/main.cjs"];
  const electronCommand = resolveElectronLaunchCommand(launchArgs);
  const app = NodeChildProcess.spawn(electronCommand.electronPath, electronCommand.args, {
    cwd: desktopDir,
    env: childEnv,
    stdio: "inherit",
  });

  currentApp = app;

  app.once("error", () => {
    if (currentApp === app) {
      currentApp = null;
    }

    restartAfterFailure(app);
  });

  app.once("exit", (code, signal) => {
    if (currentApp === app) {
      currentApp = null;
    }

    if (signal !== null || code !== 0) {
      restartAfterFailure(app);
    }
  });
}

// A spawn error can be followed by an exit event for the same child, so each child counts once.
function restartAfterFailure(app) {
  if (shuttingDown || expectedExits.has(app) || failedApps.has(app)) {
    return;
  }
  failedApps.add(app);

  if (crashLoopBreaker.recordAbnormalExit(Date.now()).tripped) {
    console.error(
      "[desktop-dev] Electron keeps crashing right after launch, so restarts are paused. " +
        "Fix the problem and rebuild to try again, or run `mise run dev:desktop:reset` " +
        "to rebuild the Electron bundle.",
    );
    return;
  }
  scheduleRestart();
}

async function stopApp() {
  const app = currentApp;
  if (!app) {
    // A crashed Electron is already gone, but its backend may still hold the T3 home.
    await stopStaleDevBackends();
    return;
  }

  currentApp = null;
  expectedExits.add(app);

  await new Promise((resolve) => {
    let settled = false;

    const finish = () => {
      if (settled) {
        return;
      }

      settled = true;
      resolve();
    };

    app.once("exit", finish);
    app.kill("SIGTERM");
    killChildTreeByPid(app.pid, "TERM");
    cleanupStaleDevApps();

    setTimeout(() => {
      if (settled) {
        return;
      }

      app.kill("SIGKILL");
      killChildTreeByPid(app.pid, "KILL");
      cleanupStaleDevApps();
      finish();
    }, forcedShutdownTimeoutMs).unref();
  });
  await stopStaleDevBackends();
}

function scheduleRestart() {
  if (shuttingDown) {
    return;
  }

  if (restartTimer) {
    clearTimeout(restartTimer);
  }

  restartTimer = setTimeout(() => {
    restartTimer = null;
    restartQueue = restartQueue
      .catch(() => undefined)
      .then(async () => {
        await stopApp();
        if (!shuttingDown) {
          startApp();
        }
      });
  }, restartDebounceMs);
}

function startWatchers() {
  for (const { directory, files } of watchedDirectories) {
    const watcher = NodeFS.watch(
      NodePath.join(desktopDir, directory),
      { persistent: true },
      (_eventType, filename) => {
        if (typeof filename !== "string" || !files.has(filename)) {
          return;
        }

        crashLoopBreaker.rearm();
        scheduleRestart();
      },
    );

    watchers.push(watcher);
  }
}

function killChildTree(signal) {
  if (hostPlatform === "win32") {
    return;
  }

  NodeChildProcess.spawnSync("pkill", [`-${signal}`, "-P", String(process.pid)], {
    stdio: "ignore",
  });
}

async function shutdown(exitCode) {
  if (shuttingDown) return;
  shuttingDown = true;

  if (restartTimer) {
    clearTimeout(restartTimer);
    restartTimer = null;
  }

  for (const watcher of watchers) {
    watcher.close();
  }

  clearInterval(orphanCheck);
  await stopApp();
  killChildTree("TERM");
  await new Promise((resolve) => {
    setTimeout(resolve, childTreeGracePeriodMs);
  });
  killChildTree("KILL");
  // Released only after teardown, because the teardown kills every Electron tagged with this
  // checkout and would take down a replacement runner's app.
  releaseRunnerRecord(desktopDir);

  process.exit(exitCode);
}

const orphanCheck = setInterval(() => {
  if (process.ppid !== initialParentPid) {
    void shutdown(0);
  }
}, orphanCheckIntervalMs);

startWatchers();
cleanupStaleDevApps();
await stopStaleDevBackends();
startApp();

process.once("SIGINT", () => {
  void shutdown(130);
});
process.once("SIGTERM", () => {
  void shutdown(143);
});
process.once("SIGHUP", () => {
  void shutdown(129);
});
