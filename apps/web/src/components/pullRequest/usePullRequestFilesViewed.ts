import type { EnvironmentId, PullRequestRef } from "@t3tools/contracts";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { pullRequestEnvironment } from "~/state/pullRequests";
import { useEnvironmentQuery } from "~/state/query";
import { useAtomCommand } from "~/state/use-atom-command";

import { toastManager } from "../ui/toast";
import {
  countViewedFiles,
  isFileViewed,
  isStaleViewedState,
  settleFileViewedOverlay,
  toFileViewedBatch,
  toFileViewedStates,
  type FileViewedOverlay,
} from "./pullRequestFilesViewed.logic";

/**
 * How long presses gather before the host is told. Long enough that ticking down a file list
 * costs one request rather than one per file, short enough that a reader who ticks one file and
 * closes the tab has already been recorded.
 */
const FLUSH_DELAY_MS = 400;

const NO_OVERLAY: FileViewedOverlay = new Map();
const NO_PATHS: ReadonlySet<string> = new Set();

export interface PullRequestFilesViewedView {
  /** Whether the host tracks this at all, which is what hides the whole control. */
  readonly enabled: boolean;
  readonly isViewed: (path: string) => boolean;
  /** The host says this file has been pushed to since it was cleared. */
  readonly isStale: (path: string) => boolean;
  readonly setViewed: (path: string, viewed: boolean) => void;
  /** How many of the files on screen are ticked off. */
  readonly viewedCount: number;
}

/**
 * Which files this reader has already cleared, as the host records it.
 *
 * The state lives on the host rather than here so a review carried on from another machine, or
 * from the host's own web UI, picks up where it was left. Presses show immediately and are held
 * over the host's answer until it agrees with them, so the checkbox never waits on a round trip.
 */
export function usePullRequestFilesViewed(options: {
  readonly environmentId: EnvironmentId;
  readonly reference: PullRequestRef;
  readonly enabled: boolean;
  /** The paths on screen, which is what the counter counts. */
  readonly paths: ReadonlyArray<string>;
}): PullRequestFilesViewedView {
  const { environmentId, reference, enabled, paths } = options;
  const query = useEnvironmentQuery(
    enabled ? pullRequestEnvironment.filesViewed({ environmentId, input: reference }) : null,
  );
  const refresh = query.refresh;
  const states = useMemo(() => toFileViewedStates(query.data), [query.data]);
  const [overlay, setOverlay] = useState<FileViewedOverlay>(NO_OVERLAY);
  const setFilesViewed = useAtomCommand(pullRequestEnvironment.setFilesViewed);

  // Presses waiting for the next flush, and the ones a request is already carrying. Both are
  // refs rather than state: nothing on screen reads them, and the flush must see the latest.
  const queued = useRef<Map<string, boolean>>(new Map());
  const inFlight = useRef<ReadonlySet<string>>(NO_PATHS);
  const flushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const referenceKey = `${reference.projectId} ${reference.repository} ${reference.number}`;
  // Everything held here is about one change request, so switching away drops it rather than
  // letting a press meant for one land on another.
  useEffect(() => {
    queued.current = new Map();
    inFlight.current = NO_PATHS;
    setOverlay(NO_OVERLAY);
  }, [referenceKey]);

  useEffect(() => {
    setOverlay((current) =>
      settleFileViewedOverlay(
        current,
        states,
        new Set([...queued.current.keys(), ...inFlight.current]),
      ),
    );
  }, [states]);

  const flush = useCallback(() => {
    flushTimer.current = null;
    const batch = toFileViewedBatch(queued.current);
    if (batch.length === 0) return;
    queued.current = new Map();
    const sent = new Set(batch.map((file) => file.path));
    inFlight.current = sent;
    void setFilesViewed({ environmentId, input: { ...reference, files: batch } }).then((result) => {
      inFlight.current = NO_PATHS;
      if (result._tag === "Failure") {
        // The host never heard these, so the ticks go back to whatever it last said.
        setOverlay((current) => {
          const next = new Map(current);
          for (const path of sent) next.delete(path);
          return next;
        });
        toastManager.add({ type: "error", title: "Could not update viewed files" });
        return;
      }
      refresh();
    });
  }, [environmentId, reference, refresh, setFilesViewed]);

  // Read through a ref rather than closed over: `setViewed` is handed to every file header the
  // viewer draws, and a new identity per render would rebuild all of them.
  const flushRef = useRef(flush);
  flushRef.current = flush;

  // A tab closed mid-gather still records what was pressed.
  useEffect(
    () => () => {
      if (flushTimer.current === null) return;
      clearTimeout(flushTimer.current);
      flushRef.current();
    },
    [],
  );

  const setViewed = useCallback((path: string, viewed: boolean) => {
    setOverlay((current) => new Map(current).set(path, viewed));
    queued.current.set(path, viewed);
    if (flushTimer.current !== null) clearTimeout(flushTimer.current);
    flushTimer.current = setTimeout(() => flushRef.current(), FLUSH_DELAY_MS);
  }, []);

  const isViewed = useCallback(
    (path: string) => isFileViewed(path, states, overlay),
    [overlay, states],
  );
  const isStale = useCallback(
    (path: string) => !overlay.has(path) && isStaleViewedState(states?.get(path)),
    [overlay, states],
  );
  const viewedCount = useMemo(
    () => countViewedFiles(paths, states, overlay),
    [overlay, paths, states],
  );

  return { enabled, isViewed, isStale, setViewed, viewedCount };
}
