import type { LiveBackgroundTask } from "@t3tools/client-runtime/state/background-work";

import { formatRelativeTimeLabel } from "~/timestampFormat";
import { Button } from "../ui/button";
import { Popover, PopoverPopup, PopoverTrigger } from "../ui/popover";

/**
 * Names the live background work behind the composer banner. Timestamps are
 * formatted at open time rather than ticking: the popup is built fresh on every
 * open, so a repainting clock would cost frames for nothing.
 */
export function BackgroundWorkDetailsPopover({
  tasks,
}: {
  readonly tasks: ReadonlyArray<LiveBackgroundTask>;
}) {
  return (
    <Popover>
      <PopoverTrigger render={<Button size="xs" variant="ghost" />}>Details</PopoverTrigger>
      <PopoverPopup align="end" side="top" className="w-80 max-w-full">
        <div className="flex flex-col gap-3">
          <ul className="flex flex-col gap-2.5">
            {tasks.map((task) => (
              <li key={task.taskId} className="flex flex-col gap-0.5">
                <div className="flex items-baseline gap-2">
                  <span className="text-[11px] text-muted-foreground uppercase tracking-wide">
                    {taskFlavorLabel(task)}
                  </span>
                  <span className="text-[11px] text-muted-foreground">
                    {formatRelativeTimeLabel(task.updatedAt)}
                  </span>
                </div>
                <span className="font-medium text-sm">{task.label}</span>
                {task.progress ? (
                  <span className="text-muted-foreground text-xs">{task.progress}</span>
                ) : null}
              </li>
            ))}
          </ul>
          <p className="text-muted-foreground text-xs">
            Stop ends everything listed here at once and interrupts the session. There is no way to
            stop a single item.
          </p>
        </div>
      </PopoverPopup>
    </Popover>
  );
}

function taskFlavorLabel(task: LiveBackgroundTask): string {
  if (task.kind === "agent") {
    return "Agent";
  }
  return task.taskType === "shell" || task.taskType === "local_bash"
    ? "Background shell"
    : "Monitor";
}
