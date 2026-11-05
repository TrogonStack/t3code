import type { PendingBackgroundWorkItem } from "@t3tools/client-runtime/state/thread-execution";
import type { EnvironmentId, ThreadId, TurnItemId } from "@t3tools/contracts";
import { ListIcon } from "lucide-react";

import { Button } from "../ui/button";
import { Popover, PopoverPopup, PopoverTrigger } from "../ui/popover";
import { CommandOutputPanel, useCommandOutputStreaming } from "./CommandOutputPanel";
import { ShellCommandBlock } from "./ShellCommandBlock";

export interface BackgroundWorkDetailRow {
  readonly task: PendingBackgroundWorkItem;
  /** Full command of the launching item, when one resolves. */
  readonly command: string | null;
  /** Set when the launching item is a command whose output the server can stream. */
  readonly outputItemId: TurnItemId | null;
}

const KIND_LABELS = {
  subagent: "Subagent",
  command: "Command",
  monitor: "Monitor",
  background_task: "Background task",
} as const;

function BackgroundWorkDetailList(props: {
  readonly environmentId: EnvironmentId;
  readonly threadId: ThreadId;
  readonly rows: ReadonlyArray<BackgroundWorkDetailRow>;
}) {
  const streamsOutput = useCommandOutputStreaming(props.environmentId);
  return (
    <div className="max-h-96 space-y-3 overflow-y-auto text-xs">
      {props.rows.map((row) => (
        <div key={row.task.taskId} className="min-w-0 space-y-1">
          <div className="text-muted-foreground">{KIND_LABELS[row.task.kind]}</div>
          {row.command ? (
            <div className="font-mono">
              <ShellCommandBlock command={row.command} />
            </div>
          ) : (
            <div className="whitespace-pre-wrap wrap-anywhere">{row.task.label}</div>
          )}
          {streamsOutput && row.outputItemId !== null ? (
            <div className="font-mono whitespace-pre-wrap wrap-anywhere">
              <CommandOutputPanel
                environmentId={props.environmentId}
                threadId={props.threadId}
                itemId={row.outputItemId}
              />
            </div>
          ) : null}
        </div>
      ))}
    </div>
  );
}

/** Pending background work with each command and its live output; mounts them only while open. */
export function BackgroundWorkDetails(props: {
  readonly environmentId: EnvironmentId;
  readonly threadId: ThreadId;
  readonly rows: ReadonlyArray<BackgroundWorkDetailRow>;
}) {
  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            size="icon-xs"
            variant="ghost-muted"
            aria-label="Show background work details"
            className="flex-none"
          />
        }
      >
        <ListIcon />
      </PopoverTrigger>
      <PopoverPopup side="top" align="start" width="lg" padding="compact">
        <BackgroundWorkDetailList {...props} />
      </PopoverPopup>
    </Popover>
  );
}
