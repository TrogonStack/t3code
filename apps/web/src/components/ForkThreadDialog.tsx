"use client";

import { Radio as RadioPrimitive } from "@base-ui/react/radio";
import { useNavigate } from "@tanstack/react-router";
import { useCallback, useState } from "react";
import {
  isAtomCommandInterrupted,
  squashAtomCommandFailure,
} from "@t3tools/client-runtime/state/runtime";
import { scopeThreadRef } from "@t3tools/client-runtime/environment";
import type { EnvironmentId, MessageId, ThreadForkMode, ThreadId } from "@t3tools/contracts";

import { cn, newThreadId } from "../lib/utils";
import { buildThreadRouteParams } from "../threadRoutes";
import { threadEnvironment } from "../state/threads";
import { useAtomCommand } from "../state/use-atom-command";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogPopup,
  DialogTitle,
} from "./ui/dialog";
import { Input } from "./ui/input";
import { RadioGroup } from "./ui/radio-group";
import { stackedThreadToast, toastManager } from "./ui/toast";

interface ForkModeOption {
  readonly value: ThreadForkMode;
  readonly label: string;
  readonly description: string;
}

const FORK_MODE_OPTIONS: readonly ForkModeOption[] = [
  {
    value: "full-history",
    label: "Full history",
    description: "Copy the entire conversation into the new thread.",
  },
  {
    value: "summary",
    label: "Summary",
    description: "Start the new thread with a condensed summary instead of the full transcript.",
  },
];

interface ForkThreadDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  environmentId: EnvironmentId;
  sourceThreadId: ThreadId;
  sourceThreadTitle: string;
  upToMessageId?: MessageId;
}

export function ForkThreadDialog({
  open,
  onOpenChange,
  environmentId,
  sourceThreadId,
  sourceThreadTitle,
  upToMessageId,
}: ForkThreadDialogProps) {
  const navigate = useNavigate();
  const forkThread = useAtomCommand(threadEnvironment.fork, { reportFailure: false });

  const [mode, setMode] = useState<ThreadForkMode>("full-history");
  const [title, setTitle] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSave = useCallback(async () => {
    setIsSubmitting(true);
    const trimmedTitle = title.trim();
    const forkedThreadId = newThreadId();
    const result = await forkThread({
      environmentId,
      input: {
        sourceThreadId,
        newThreadId: forkedThreadId,
        mode,
        ...(upToMessageId !== undefined ? { upToMessageId } : {}),
        ...(trimmedTitle.length > 0 ? { title: trimmedTitle } : {}),
      },
    });
    setIsSubmitting(false);

    if (result._tag === "Failure") {
      if (isAtomCommandInterrupted(result)) {
        return;
      }
      const error = squashAtomCommandFailure(result);
      toastManager.add(
        stackedThreadToast({
          type: "error",
          title: "Could not fork thread",
          description: error instanceof Error ? error.message : "An error occurred.",
        }),
      );
      return;
    }

    onOpenChange(false);
    toastManager.add({
      type: "success",
      title: "Thread forked",
      description: `Created a new thread from "${sourceThreadTitle}".`,
    });
    await navigate({
      to: "/$environmentId/$threadId",
      params: buildThreadRouteParams(scopeThreadRef(environmentId, forkedThreadId)),
    });
  }, [
    environmentId,
    forkThread,
    mode,
    navigate,
    onOpenChange,
    sourceThreadId,
    sourceThreadTitle,
    title,
    upToMessageId,
  ]);

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!isSubmitting) {
          onOpenChange(nextOpen);
        }
      }}
    >
      <DialogPopup className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Fork thread</DialogTitle>
          <DialogDescription>
            {upToMessageId
              ? `Create a new thread from "${sourceThreadTitle}", up to this message.`
              : `Create a new thread from "${sourceThreadTitle}".`}
          </DialogDescription>
        </DialogHeader>
        <DialogPanel className="space-y-4">
          <div className="grid gap-2">
            <span id="fork-thread-mode-label" className="text-xs font-medium text-foreground">
              Mode
            </span>
            <RadioGroup
              value={mode}
              onValueChange={(value) => setMode(value as ThreadForkMode)}
              aria-labelledby="fork-thread-mode-label"
              className="grid gap-2.5"
            >
              {FORK_MODE_OPTIONS.map((option) => {
                const isSelected = option.value === mode;
                return (
                  <RadioPrimitive.Root
                    key={option.value}
                    value={option.value}
                    className={cn(
                      "relative flex cursor-pointer flex-col gap-1 rounded-lg border px-3 py-3 text-left outline-none transition-[background-color,border-color,box-shadow]",
                      "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background",
                      isSelected
                        ? "border-primary bg-background shadow-sm ring-2 ring-primary/35"
                        : "border-border bg-background hover:border-foreground/20 hover:bg-muted/50",
                    )}
                  >
                    <span className="text-sm font-medium text-foreground">{option.label}</span>
                    <span className="text-xs text-muted-foreground">{option.description}</span>
                  </RadioPrimitive.Root>
                );
              })}
            </RadioGroup>
          </div>

          <label className="grid gap-1.5">
            <span className="text-xs font-medium text-foreground">Title</span>
            <Input
              placeholder={`${sourceThreadTitle} (fork)`}
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void handleSave();
                }
              }}
            />
            <span className="text-[11px] text-muted-foreground">
              Optional. Leave blank to use a default title.
            </span>
          </label>
        </DialogPanel>
        <DialogFooter>
          <Button
            variant="outline"
            size="sm"
            onClick={() => onOpenChange(false)}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
          <Button size="sm" onClick={() => void handleSave()} disabled={isSubmitting}>
            {isSubmitting ? "Forking..." : "Fork thread"}
          </Button>
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  );
}
