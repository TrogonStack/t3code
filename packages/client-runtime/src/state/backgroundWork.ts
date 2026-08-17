/**
 * Per-task detail for a thread's live background work, folded from the same
 * persisted task.* rows the server's liveness registry is fed.
 *
 * The registry answers one question for the whole thread ("working",
 * "monitoring", or nothing) and keeps no per-task detail, so a client that
 * only reads it can name a state but never the work. This fold recovers the
 * individual tasks so the shell can say what is running and what stopping it
 * would end.
 *
 * It mirrors ThreadBackgroundLiveness rather than inventing a second opinion:
 * the same contracts classification sets, the same drop rules (inert types,
 * agent-internal watch loops, terminal and idle statuses), and the same
 * per-transition classification, so this list can never claim work the
 * registry does not hold live. Metadata is the one sticky part, because a
 * task's title usually arrives only on its start row while terminal rows
 * commonly carry taskId and status alone.
 *
 * Retention is why callers must read an empty list as "no detail available"
 * rather than "nothing running": start rows age out, and after a server
 * restart the registry is empty while old rows survive. The registry stays
 * the authority on whether background work exists at all.
 */
import { INERT_TASK_TYPES, MONITOR_TASK_TYPES } from "@t3tools/contracts";
import type { OrchestrationThreadActivity } from "@t3tools/contracts";

/** Watch loops (Monitor tasks, background shells) vs everything else live. */
export type LiveBackgroundTaskKind = "watch" | "agent";

export interface LiveBackgroundTask {
  readonly taskId: string;
  readonly kind: LiveBackgroundTaskKind;
  /** SDK task_type when a row carried one, for callers that label by flavor. */
  readonly taskType: string | null;
  /** Best available human label, falling back to the task id. */
  readonly label: string;
  /** Latest progress line, which is what "following" a watch loop means. */
  readonly progress: string | null;
  readonly firstSeenAt: string;
  readonly updatedAt: string;
}

const TASK_LIFECYCLE_KINDS: ReadonlySet<string> = new Set([
  "task.started",
  "task.progress",
  "task.updated",
  "task.completed",
]);

// Registry copy, including idle as not-live: a resting (resumable) child is
// not doing anything.
const SETTLED_STATUSES: ReadonlySet<string> = new Set([
  "completed",
  "failed",
  "stopped",
  "cancelled",
  "interrupted",
  "idle",
]);

interface TaskState {
  kind: LiveBackgroundTaskKind | null;
  taskType: string | null;
  label: string | null;
  progress: string | null;
  firstSeenAt: string;
  updatedAt: string;
}

// Same bound the subagent fold uses: task text is provider-supplied and a
// banner or popover line is not the place to discover it was a page long.
const TEXT_CHAR_LIMIT = 180;

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

/** Display text: identity fields keep asString, since truncating an id lies. */
function asText(value: unknown): string | undefined {
  const text = asString(value);
  if (text === undefined) {
    return undefined;
  }
  return text.length <= TEXT_CHAR_LIMIT ? text : `${text.slice(0, TEXT_CHAR_LIMIT - 1)}…`;
}

function compareByOrder(
  left: OrchestrationThreadActivity,
  right: OrchestrationThreadActivity,
): number {
  if (
    left.sequence !== undefined &&
    right.sequence !== undefined &&
    left.sequence !== right.sequence
  ) {
    return left.sequence - right.sequence;
  }
  const byCreatedAt = left.createdAt.localeCompare(right.createdAt);
  return byCreatedAt !== 0 ? byCreatedAt : left.id.localeCompare(right.id);
}

/**
 * Folds task rows into the background work still live, oldest first. Pure —
 * memoize by activity-list identity at the atom layer.
 */
export function foldLiveBackgroundTasks(
  activities: ReadonlyArray<OrchestrationThreadActivity>,
): ReadonlyArray<LiveBackgroundTask> {
  const tasks = new Map<string, TaskState>();

  for (const activity of [...activities].toSorted(compareByOrder)) {
    if (!TASK_LIFECYCLE_KINDS.has(activity.kind)) continue;
    if (typeof activity.payload !== "object" || activity.payload === null) continue;
    const payload = activity.payload as Record<string, unknown>;
    const taskId = asString(payload.taskId);
    if (!taskId) continue;

    const taskType = asString(payload.taskType);
    const agentId = asString(payload.agentId);
    const status = asString(payload.status);
    const at = activity.createdAt;
    const existing = tasks.get(taskId);
    const state: TaskState = {
      kind: null,
      taskType: taskType ?? existing?.taskType ?? null,
      // An explicit title always wins; otherwise keep what we know rather
      // than letting a thinner later row downgrade the label.
      label:
        asText(payload.title) ??
        existing?.label ??
        asText(payload.description) ??
        asText(payload.detail) ??
        null,
      progress: asText(payload.summary) ?? existing?.progress ?? null,
      firstSeenAt: existing?.firstSeenAt ?? at,
      updatedAt: at,
    };
    tasks.set(taskId, state);

    // Drop rules in the registry's order. Classification is per-transition,
    // not sticky: a task whose taskType only shows up on a later row must be
    // reclassified rather than pinned by its first row.
    if (taskType !== undefined && INERT_TASK_TYPES.has(taskType)) continue;
    // A subagent's own watch loops are covered by the owning agent's
    // liveness. A nested agent falls through: it can outlive its parent.
    if (agentId !== undefined && (taskType === undefined || MONITOR_TASK_TYPES.has(taskType))) {
      continue;
    }
    if (activity.kind === "task.completed") continue;
    if (status !== undefined && SETTLED_STATUSES.has(status)) continue;

    // Whether a task is live mirrors the registry exactly (the rules above
    // read this row's own fields). Which flavor it is reads the sticky type:
    // losing taskType on a thinner later row is not news about the work, and
    // flavor only decides wording, never presence.
    state.kind =
      state.taskType !== null && MONITOR_TASK_TYPES.has(state.taskType) ? "watch" : "agent";
  }

  const live: LiveBackgroundTask[] = [];
  for (const [taskId, state] of tasks) {
    if (state.kind === null) continue;
    live.push({
      taskId,
      kind: state.kind,
      taskType: state.taskType,
      label: state.label ?? taskId,
      progress: state.progress,
      firstSeenAt: state.firstSeenAt,
      updatedAt: state.updatedAt,
    });
  }
  return live.toSorted((left, right) => left.firstSeenAt.localeCompare(right.firstSeenAt));
}
