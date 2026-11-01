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

export interface FileDiffFoldDefaults {
  readonly defaultCollapsed: boolean;
  readonly viewed: boolean;
}

function isFileDiffFoldedByDefault(
  foldOverride: DiffFoldOverride,
  { defaultCollapsed, viewed }: FileDiffFoldDefaults,
): boolean {
  // Viewed files fold until the toolbar asks for every file one way.
  return foldOverride === null ? defaultCollapsed || viewed : foldOverride === "folded";
}

/** Reader choices are absolute so a later viewed refresh cannot flip them. */
export function isFileDiffCollapsed(
  fileKey: string,
  foldOverride: DiffFoldOverride,
  foldChoices: ReadonlyMap<string, boolean>,
  defaults: FileDiffFoldDefaults,
): boolean {
  return foldChoices.get(fileKey) ?? isFileDiffFoldedByDefault(foldOverride, defaults);
}

export function setFileDiffFold(
  foldChoices: ReadonlyMap<string, boolean>,
  fileKey: string,
  collapsed: boolean,
): ReadonlyMap<string, boolean> {
  if (foldChoices.get(fileKey) === collapsed) return foldChoices;
  return new Map(foldChoices).set(fileKey, collapsed);
}

/** Lean on the default where it agrees, so a file pushed to after viewing unfolds again. */
export function foldFileDiffForViewed(
  foldChoices: ReadonlyMap<string, boolean>,
  fileKey: string,
  viewed: boolean,
  foldOverride: DiffFoldOverride,
  defaultCollapsed: boolean,
): ReadonlyMap<string, boolean> {
  if (isFileDiffFoldedByDefault(foldOverride, { defaultCollapsed, viewed }) !== viewed) {
    return setFileDiffFold(foldChoices, fileKey, viewed);
  }
  if (!foldChoices.has(fileKey)) return foldChoices;
  const next = new Map(foldChoices);
  next.delete(fileKey);
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
