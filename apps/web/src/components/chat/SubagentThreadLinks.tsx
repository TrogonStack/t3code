import type { EnvironmentId, ThreadId } from "@t3tools/contracts";
import { scopeThreadRef } from "@t3tools/client-runtime/environment";
import { useNavigate } from "@tanstack/react-router";
import { BotIcon, CornerLeftUpIcon } from "lucide-react";
import { useMemo } from "react";

import { useShallow } from "zustand/react/shallow";

import { useThreadShells } from "~/state/entities";
import { useUiStateStore } from "~/uiStateStore";
import { resolveThreadStatusPill } from "../Sidebar.logic";
import { scopedThreadKey } from "@t3tools/client-runtime/environment";
import { buildThreadRouteParams } from "~/threadRoutes";
import { cn } from "~/lib/utils";
import { Menu, MenuItem, MenuPopup, MenuTrigger } from "../ui/menu";
import { Tooltip, TooltipPopup, TooltipTrigger } from "../ui/tooltip";

interface SubagentThreadLinksProps {
  readonly environmentId: EnvironmentId;
  readonly threadId: ThreadId;
  readonly parentThreadId: ThreadId | null;
}

const subagentStatusPill = (
  shell: Parameters<typeof resolveThreadStatusPill>[0]["thread"],
  lastVisitedAt: string | undefined,
) =>
  resolveThreadStatusPill({
    thread: { ...shell, ...(lastVisitedAt !== undefined ? { lastVisitedAt } : {}) },
  });

export function SubagentThreadLinks({
  environmentId,
  threadId,
  parentThreadId,
}: SubagentThreadLinksProps) {
  const navigate = useNavigate();
  const threadShells = useThreadShells();
  const threadLastVisitedAtById = useUiStateStore(
    useShallow((state) => state.threadLastVisitedAtById),
  );

  const parentShell = useMemo(
    () =>
      parentThreadId === null
        ? undefined
        : threadShells.find(
            (shell) => shell.environmentId === environmentId && shell.id === parentThreadId,
          ),
    [environmentId, parentThreadId, threadShells],
  );
  const children = useMemo(
    () =>
      threadShells.filter(
        (shell) =>
          shell.environmentId === environmentId && (shell.parentThreadId ?? null) === threadId,
      ),
    [environmentId, threadId, threadShells],
  );

  const openThread = (targetThreadId: ThreadId) =>
    void navigate({
      to: "/$environmentId/$threadId",
      params: buildThreadRouteParams(scopeThreadRef(environmentId, targetThreadId)),
    });

  if (parentShell !== undefined) {
    return (
      <Tooltip>
        <TooltipTrigger
          render={
            <button
              type="button"
              onClick={() => openThread(parentShell.id)}
              className="flex shrink-0 cursor-pointer items-center gap-1 rounded-full border border-border/60 px-2 py-0.5 text-muted-foreground text-xs transition-colors hover:border-border hover:text-foreground"
            >
              <CornerLeftUpIcon className="size-3" />
              <span className="max-w-40 truncate">{parentShell.title}</span>
            </button>
          }
        />
        <TooltipPopup side="bottom">Spawned by "{parentShell.title}"</TooltipPopup>
      </Tooltip>
    );
  }

  if (children.length === 0) {
    return null;
  }

  const runningCount = children.filter(
    (shell) => shell.session?.status === "running" || shell.session?.status === "starting",
  ).length;
  return (
    <Menu>
      <MenuTrigger className="flex shrink-0 cursor-pointer items-center gap-1.5 rounded-full border border-border/60 px-2 py-0.5 text-muted-foreground text-xs transition-colors hover:border-border hover:text-foreground">
        <BotIcon className="size-3" />
        {children.length} subagent{children.length === 1 ? "" : "s"}
        {runningCount > 0 ? (
          <span className="size-1.5 animate-pulse rounded-full bg-blue-500" aria-hidden="true" />
        ) : null}
      </MenuTrigger>
      <MenuPopup align="start" className="w-72">
        {children.map((shell) => {
          const pill = subagentStatusPill(
            shell,
            threadLastVisitedAtById[scopedThreadKey(scopeThreadRef(environmentId, shell.id))],
          );
          return (
            <MenuItem key={shell.id} onClick={() => openThread(shell.id)}>
              <span className="flex min-w-0 flex-1 items-center gap-2">
                <span
                  aria-hidden="true"
                  className={cn(
                    "size-1.5 shrink-0 rounded-full",
                    pill === null ? "bg-muted-foreground/30" : pill.dotClass,
                    pill?.pulse && "animate-pulse",
                  )}
                />
                <span className="min-w-0 flex-1 truncate">{shell.title}</span>
                {pill !== null && (
                  <span className={cn("shrink-0 text-[10px]", pill.colorClass)}>{pill.label}</span>
                )}
              </span>
            </MenuItem>
          );
        })}
      </MenuPopup>
    </Menu>
  );
}
