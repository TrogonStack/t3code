// @effect-diagnostics nodeBuiltinImport:off
import * as NodeFS from "node:fs";
import * as NodeOS from "node:os";
import * as NodePath from "node:path";
import * as NodeURL from "node:url";

import * as NodeServices from "@effect/platform-node/NodeServices";
import { it } from "@effect/vitest";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import { describe, expect } from "vite-plus/test";

import * as AcpSessionRuntime from "./AcpSessionRuntime.ts";

const __dirname = NodePath.dirname(NodeURL.fileURLToPath(import.meta.url));
const mockAgentPath = NodePath.join(__dirname, "../../../scripts/acp-mock-agent.ts");

// These drive a real subprocess on the live clock, so `it.live` rather than
// `it.effect`. The watchdog polls once a second, so a stall needs at least two
// polls to be observed: the timeout sits above one poll and the handler holds
// the request across several.
const testStallTimeout = Duration.millis(1500);
const handlerHoldDuration = Duration.seconds(4);

const makeRuntime = (env: NodeJS.ProcessEnv) =>
  AcpSessionRuntime.make({
    spawn: {
      command: process.execPath,
      args: [mockAgentPath],
      env,
    },
    cwd: process.cwd(),
    clientInfo: { name: "t3-test", version: "0.0.0" },
    authMethodId: "test",
    promptStallTimeout: testStallTimeout,
  });

describe("AcpSessionRuntime prompt stall detection", () => {
  it.live("keeps waiting while an extension question is parked on the user", () =>
    Effect.gen(function* () {
      const runtime = yield* makeRuntime({ T3_ACP_EMIT_ASK_QUESTION: "1" });
      // A real user takes as long as they take. The agent is not silent because
      // it died, it is silent because it is waiting on this answer.
      yield* runtime.handleExtRequest("cursor/ask_question", Schema.Unknown, () =>
        Effect.sleep(handlerHoldDuration).pipe(
          Effect.as({ answers: [{ id: "scope", value: "workspace" }] }),
        ),
      );
      yield* runtime.start();

      const result = yield* runtime.prompt({ prompt: [{ type: "text", text: "hi" }] });

      expect(result).toMatchObject({ stopReason: "end_turn" });
    }).pipe(Effect.scoped, Effect.provide(NodeServices.layer)),
  );

  it.live("fails the prompt once the agent goes silent with nothing outstanding", () =>
    Effect.gen(function* () {
      const runtime = yield* makeRuntime({ T3_ACP_HANG_PROMPT_FOREVER: "1" });
      yield* runtime.start();

      const error = yield* runtime
        .prompt({ prompt: [{ type: "text", text: "hi" }] })
        .pipe(Effect.flip);

      expect(error._tag).toBe("AcpTransportError");
      expect(error).toMatchObject({ method: "session/prompt" });
      expect(String((error as { detail?: string }).detail)).toContain("stalled");
    }).pipe(Effect.scoped, Effect.provide(NodeServices.layer)),
  );

  it.live("does not accept a chattering child session as proof this prompt is alive", () =>
    Effect.gen(function* () {
      const runtime = yield* makeRuntime({
        T3_ACP_HANG_PROMPT_FOREVER: "1",
        T3_ACP_EMIT_CHILD_UPDATES_WHILE_HANGING: "1",
      });
      yield* runtime.start();

      const error = yield* runtime
        .prompt({ prompt: [{ type: "text", text: "hi" }] })
        .pipe(Effect.flip);

      expect(error._tag).toBe("AcpTransportError");
    }).pipe(Effect.scoped, Effect.provide(NodeServices.layer)),
  );

  it.live("cancels the wedged prompt so the agent can release it", () =>
    Effect.gen(function* () {
      const requestLogPath = NodePath.join(
        yield* Effect.sync(() => NodeFS.mkdtempSync(NodePath.join(NodeOS.tmpdir(), "t3-acp-"))),
        "requests.ndjson",
      );
      const runtime = yield* makeRuntime({
        T3_ACP_HANG_PROMPT_FOREVER: "1",
        T3_ACP_REQUEST_LOG_PATH: requestLogPath,
      });
      yield* runtime.start();

      yield* runtime.prompt({ prompt: [{ type: "text", text: "hi" }] }).pipe(Effect.flip);
      // The notification is fired off as the prompt fails, so give the write a
      // moment to land before reading the agent's view of what it received.
      yield* Effect.sleep(Duration.millis(250));

      const received = yield* Effect.sync(() => NodeFS.readFileSync(requestLogPath, "utf8"));
      expect(received).toContain("session/cancel");
    }).pipe(Effect.scoped, Effect.provide(NodeServices.layer)),
  );
});
