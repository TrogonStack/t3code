import type { FileDiffMetadata } from "@pierre/diffs";
import type { PullRequestDiffSide } from "@t3tools/contracts";

/** Only rendered hunks can anchor comments; list others off-diff. */
export function isLineInFileDiff(
  file: FileDiffMetadata,
  side: PullRequestDiffSide,
  line: number,
): boolean {
  return file.hunks.some((hunk) =>
    side === "left"
      ? line >= hunk.deletionStart && line < hunk.deletionStart + hunk.deletionCount
      : line >= hunk.additionStart && line < hunk.additionStart + hunk.additionCount,
  );
}

export type DiffFoldOverride = "expanded" | "folded" | null;

/** The toolbar's choice wins; until then viewed files fold whatever the default. */
export function isFileDiffFoldedByDefault(
  foldOverride: DiffFoldOverride,
  defaultCollapsed: boolean,
  viewed: boolean,
): boolean {
  if (foldOverride !== null) return foldOverride === "folded";
  return defaultCollapsed || viewed;
}

/** Keep fold choices relative to the default so later pages inherit Fold all. */
export function isFileDiffCollapsed(
  fileKey: string,
  foldedByDefault: boolean,
  toggledFileKeys: ReadonlySet<string>,
): boolean {
  return toggledFileKeys.has(fileKey) ? !foldedByDefault : foldedByDefault;
}

export function setFileDiffCollapsed(
  fileKey: string,
  collapsed: boolean,
  foldedByDefault: boolean,
  toggledFileKeys: ReadonlySet<string>,
): ReadonlySet<string> {
  if (isFileDiffCollapsed(fileKey, foldedByDefault, toggledFileKeys) === collapsed)
    return toggledFileKeys;
  const next = new Set(toggledFileKeys);
  if (next.has(fileKey)) next.delete(fileKey);
  else next.add(fileKey);
  return next;
}

export function getFileViewedRange<T extends { readonly fileKey: string }>(
  files: ReadonlyArray<T>,
  fileKey: string,
  anchorFileKey: string | null,
): ReadonlyArray<T> {
  const index = files.findIndex((file) => file.fileKey === fileKey);
  if (index === -1) return [];
  const anchorIndex = files.findIndex((file) => file.fileKey === anchorFileKey);
  if (anchorIndex === -1) return files.slice(index, index + 1);
  return files.slice(Math.min(index, anchorIndex), Math.max(index, anchorIndex) + 1);
}
