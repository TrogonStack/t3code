import type { FileDiffMetadata } from "@pierre/diffs";
import { describe, expect, it } from "vite-plus/test";

import {
  foldFileDiffForViewed,
  getFileViewedRange,
  isFileDiffCollapsed,
  isLineInFileDiff,
} from "./pullRequestDiff.logic";

function fileWithHunks(
  hunks: ReadonlyArray<{
    deletionStart: number;
    deletionCount: number;
    additionStart: number;
    additionCount: number;
  }>,
): FileDiffMetadata {
  return { name: "src/app.ts", hunks } as unknown as FileDiffMetadata;
}

describe("isLineInFileDiff", () => {
  const file = fileWithHunks([
    { deletionStart: 10, deletionCount: 3, additionStart: 10, additionCount: 5 },
    { deletionStart: 40, deletionCount: 0, additionStart: 42, additionCount: 2 },
  ]);

  it("includes the first line of a hunk and excludes the one past its last", () => {
    expect(isLineInFileDiff(file, "right", 10)).toBe(true);
    expect(isLineInFileDiff(file, "right", 14)).toBe(true);
    expect(isLineInFileDiff(file, "right", 15)).toBe(false);
    expect(isLineInFileDiff(file, "left", 9)).toBe(false);
    expect(isLineInFileDiff(file, "left", 12)).toBe(true);
    expect(isLineInFileDiff(file, "left", 13)).toBe(false);
  });

  it("keeps the two sides apart, since one line number means two lines", () => {
    expect(isLineInFileDiff(file, "right", 43)).toBe(true);
    expect(isLineInFileDiff(file, "left", 40)).toBe(false);
  });
});

describe("isFileDiffCollapsed", () => {
  const expandedUnviewed = { defaultCollapsed: false, viewed: false };

  // regression: #9174 diffs now expand by default; a reader's fold must survive new slices
  it("keeps a file the reader folded closed as the next slice arrives", () => {
    const choices = new Map([["b.ts", true]]);
    expect(isFileDiffCollapsed("b.ts", null, choices, expandedUnviewed)).toBe(true);
    expect(isFileDiffCollapsed("c.ts", null, choices, expandedUnviewed)).toBe(false);
  });

  it("folds a file viewed in an earlier visit even when diffs expand by default", () => {
    const viewed = { defaultCollapsed: false, viewed: true };
    expect(isFileDiffCollapsed("a.ts", null, new Map(), viewed)).toBe(true);
  });

  it("keeps the reader's fold when viewed state changes underneath it", () => {
    const viewed = { defaultCollapsed: false, viewed: true };
    expect(isFileDiffCollapsed("a.ts", null, new Map([["a.ts", false]]), viewed)).toBe(false);
    expect(isFileDiffCollapsed("a.ts", null, new Map([["a.ts", true]]), expandedUnviewed)).toBe(
      true,
    );
  });

  it("lets Expand all open viewed files", () => {
    const viewed = { defaultCollapsed: true, viewed: true };
    expect(isFileDiffCollapsed("a.ts", "expanded", new Map(), viewed)).toBe(false);
  });
});

describe("foldFileDiffForViewed", () => {
  it("records a choice only against the toolbar", () => {
    expect(foldFileDiffForViewed(new Map(), "a.ts", true, null, false).size).toBe(0);
    expect([...foldFileDiffForViewed(new Map(), "a.ts", true, "expanded", false)]).toEqual([
      ["a.ts", true],
    ]);
  });

  it("drops an earlier unfold so the viewed file folds and unfolds again once pushed to", () => {
    expect(foldFileDiffForViewed(new Map([["a.ts", false]]), "a.ts", true, null, false).size).toBe(
      0,
    );
  });
});

describe("getFileViewedRange", () => {
  const files = ["c.ts", "a.ts", "b.ts", "d.ts"].map((fileKey) => ({ fileKey }));

  it("includes both endpoints in displayed order in either direction", () => {
    expect(getFileViewedRange(files, "b.ts", "c.ts")).toEqual(files.slice(0, 3));
    expect(getFileViewedRange(files, "c.ts", "b.ts")).toEqual(files.slice(0, 3));
    expect(getFileViewedRange(files, "a.ts", "a.ts")).toEqual([files[1]]);
  });

  it("changes only the clicked file without a current anchor", () => {
    expect(getFileViewedRange(files, "a.ts", null)).toEqual([files[1]]);
    expect(getFileViewedRange(files, "a.ts", "removed.ts")).toEqual([files[1]]);
    expect(getFileViewedRange(files, "removed.ts", "a.ts")).toEqual([]);
  });
});
