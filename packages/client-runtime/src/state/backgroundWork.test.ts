import { describe, expect, it } from "vite-plus/test";
import type { OrchestrationThreadActivity } from "@t3tools/contracts";
import { foldLiveBackgroundTasks } from "./backgroundWork.ts";

let sequence = 0;
function activity(
  kind: string,
  payload: Record<string, unknown>,
  at = `2026-08-01T10:00:${String(sequence).padStart(2, "0")}.000Z`,
): OrchestrationThreadActivity {
  sequence += 1;
  return {
    id: `activity-${sequence}`,
    tone: "info",
    kind,
    summary: kind,
    payload,
    turnId: null,
    createdAt: at,
  } as unknown as OrchestrationThreadActivity;
}

describe("foldLiveBackgroundTasks", () => {
  it("names a live watch loop and keeps its latest progress line", () => {
    const live = foldLiveBackgroundTasks([
      activity("task.started", {
        taskId: "t1",
        taskType: "monitor",
        title: "Watch CI on PR #18",
      }),
      activity("task.progress", { taskId: "t1", summary: "3 checks pending" }),
      activity("task.progress", { taskId: "t1", summary: "2 checks pending" }),
    ]);

    expect(live).toEqual([
      {
        taskId: "t1",
        kind: "watch",
        taskType: "monitor",
        label: "Watch CI on PR #18",
        progress: "2 checks pending",
        firstSeenAt: expect.any(String),
        updatedAt: expect.any(String),
      },
    ]);
  });

  it("separates watch loops from agent work", () => {
    const live = foldLiveBackgroundTasks([
      activity("task.started", { taskId: "t1", taskType: "monitor", title: "Tail logs" }),
      activity("task.started", { taskId: "t2", taskType: "subagent", title: "Review the diff" }),
    ]);

    expect(live.map((task) => [task.taskId, task.kind])).toEqual([
      ["t1", "watch"],
      ["t2", "agent"],
    ]);
  });

  it("drops a task once it completes, fails, is stopped, or goes idle", () => {
    const settled = ["failed", "stopped", "cancelled", "interrupted", "idle"];
    for (const status of settled) {
      const live = foldLiveBackgroundTasks([
        activity("task.started", { taskId: "t1", taskType: "monitor", title: "Watch" }),
        activity("task.updated", { taskId: "t1", status }),
      ]);
      expect(live, `status ${status}`).toEqual([]);
    }

    expect(
      foldLiveBackgroundTasks([
        activity("task.started", { taskId: "t1", taskType: "monitor", title: "Watch" }),
        activity("task.completed", { taskId: "t1" }),
      ]),
    ).toEqual([]);
  });

  it("brings a task back when a later row reports it running again", () => {
    const live = foldLiveBackgroundTasks([
      activity("task.started", { taskId: "t1", taskType: "shell", title: "Tail logs" }),
      activity("task.updated", { taskId: "t1", status: "idle" }),
      activity("task.progress", { taskId: "t1", taskType: "shell", status: "running" }),
    ]);

    expect(live.map((task) => task.taskId)).toEqual(["t1"]);
  });

  it("ignores plan-mode bookkeeping, which is neither agent nor watch loop", () => {
    expect(
      foldLiveBackgroundTasks([
        activity("task.started", { taskId: "t1", taskType: "plan", title: "Plan mode" }),
        activity("task.started", { taskId: "t2", taskType: "dream", title: "Dreaming" }),
      ]),
    ).toEqual([]);
  });

  it("ignores a subagent's own watch loops, which its owner already covers", () => {
    const live = foldLiveBackgroundTasks([
      activity("task.started", {
        taskId: "t1",
        taskType: "shell",
        agentId: "agent-1",
        title: "Agent's own shell",
      }),
      activity("task.started", {
        taskId: "t2",
        agentId: "agent-1",
        title: "Untyped agent-internal work",
      }),
      activity("task.started", {
        taskId: "t3",
        taskType: "subagent",
        agentId: "agent-1",
        title: "Nested agent",
      }),
    ]);

    expect(live.map((task) => task.taskId)).toEqual(["t3"]);
  });

  it("reclassifies when the task type only shows up on a later row", () => {
    const live = foldLiveBackgroundTasks([
      activity("task.started", { taskId: "t1", title: "Something" }),
      activity("task.progress", { taskId: "t1", taskType: "monitor" }),
    ]);

    expect(live.map((task) => task.kind)).toEqual(["watch"]);
  });

  it("keeps a watch loop's flavor when a thinner later row omits the task type", () => {
    const live = foldLiveBackgroundTasks([
      activity("task.started", { taskId: "t1", taskType: "monitor", title: "Watch CI" }),
      activity("task.progress", { taskId: "t1", summary: "still waiting" }),
    ]);

    expect(live.map((task) => task.kind)).toEqual(["watch"]);
  });

  it("keeps the label when a thinner later row omits it", () => {
    const live = foldLiveBackgroundTasks([
      activity("task.started", { taskId: "t1", taskType: "monitor", title: "Watch CI" }),
      activity("task.progress", { taskId: "t1" }),
    ]);

    expect(live[0]?.label).toBe("Watch CI");
  });

  it("falls back to the task id when no row carried a label", () => {
    const live = foldLiveBackgroundTasks([
      activity("task.progress", { taskId: "t1", taskType: "monitor" }),
    ]);

    expect(live[0]?.label).toBe("t1");
  });

  it("folds out-of-order rows by sequence, so a late start cannot reopen a task", () => {
    const started = {
      ...activity("task.started", { taskId: "t1", taskType: "monitor" }),
      sequence: 1,
    };
    const completed = { ...activity("task.completed", { taskId: "t1" }), sequence: 2 };

    expect(foldLiveBackgroundTasks([completed, started])).toEqual([]);
  });

  it("skips rows without a task id or payload", () => {
    expect(
      foldLiveBackgroundTasks([
        activity("task.started", { taskType: "monitor" }),
        activity("message.appended", { taskId: "t1", taskType: "monitor" }),
        { ...activity("task.started", {}), payload: null } as OrchestrationThreadActivity,
      ]),
    ).toEqual([]);
  });
});
