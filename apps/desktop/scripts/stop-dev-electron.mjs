import * as NodePath from "node:path";
import * as NodeURL from "node:url";

import { stopRecordedRunner } from "./dev-runner-control.mjs";

const desktopDir = NodePath.resolve(NodePath.dirname(NodeURL.fileURLToPath(import.meta.url)), "..");

const result = await stopRecordedRunner(desktopDir);
switch (result.action) {
  case "stop":
    console.log(`[desktop-dev] stopped runner ${result.pid} for this checkout`);
    break;
  case "stale":
    console.log(`[desktop-dev] removed a stale runner.json for pid ${result.pid}`);
    break;
  case "none":
    console.log("[desktop-dev] no runner is running for this checkout");
    break;
  case "unverifiable":
    console.error(
      `[desktop-dev] runner ${result.pid} may own this checkout but its working directory could not be verified; stop it manually`,
    );
    process.exitCode = 1;
    break;
  case "unsupported":
    console.error(
      "[desktop-dev] stopping the runner is not supported on Windows; stop it manually",
    );
    process.exitCode = 1;
    break;
}
