import * as NodeOS from "node:os";
import * as NodePath from "node:path";
import * as NodeURL from "node:url";

import { releaseRunnerRecord, stopPreviousRunner } from "./dev-runner-control.mjs";

const desktopDir = NodePath.resolve(NodePath.dirname(NodeURL.fileURLToPath(import.meta.url)), "..");

// oxlint-disable-next-line t3code/no-global-process-runtime -- Standalone dev script has no Effect runtime.
if (NodeOS.platform() === "win32") {
  console.error("[desktop-dev] stopping the runner is not supported on Windows; stop it manually");
  process.exit(1);
}

const { stoppedPid, refusedPid } = await stopPreviousRunner(desktopDir, 0);
if (stoppedPid !== null) {
  releaseRunnerRecord(desktopDir, stoppedPid);
  console.log(`[desktop-dev] stopped runner ${stoppedPid} for this checkout`);
} else if (refusedPid) {
  console.log(
    `[desktop-dev] runner.json names pid ${refusedPid}, which is not a runner for this checkout; left it alone`,
  );
} else {
  console.log("[desktop-dev] no runner is running for this checkout");
}
