import { describe, expect, it } from "vite-plus/test";

import {
  countViewedFiles,
  isFileViewed,
  isStaleViewedState,
  settleFileViewedOverlay,
  toFileViewedBatch,
  toFileViewedStates,
  type FileViewedOverlay,
} from "./pullRequestFilesViewed.logic";

const NO_OVERLAY: FileViewedOverlay = new Map();
const NOTHING_PENDING: ReadonlySet<string> = new Set();

const states = toFileViewedStates({
  files: [
    { path: "a.ts", state: "viewed" },
    { path: "b.ts", state: "unviewed" },
    { path: "c.ts", state: "dismissed" },
  ],
  truncated: false,
});

describe("isFileViewed", () => {
  it("follows the host for a file the reader has not pressed", () => {
    expect(isFileViewed("a.ts", states, NO_OVERLAY)).toBe(true);
    expect(isFileViewed("b.ts", states, NO_OVERLAY)).toBe(false);
  });

  it("reads a file pushed to since it was cleared as unread", () => {
    expect(isFileViewed("c.ts", states, NO_OVERLAY)).toBe(false);
    expect(isStaleViewedState(states?.get("c.ts"))).toBe(true);
    expect(isStaleViewedState(states?.get("a.ts"))).toBe(false);
  });

  it("shows the press ahead of the host's answer", () => {
    expect(isFileViewed("b.ts", states, new Map([["b.ts", true]]))).toBe(true);
    expect(isFileViewed("a.ts", states, new Map([["a.ts", false]]))).toBe(false);
  });

  it("answers a file the host has said nothing about, before its answer arrives", () => {
    expect(isFileViewed("z.ts", null, NO_OVERLAY)).toBe(false);
    expect(isFileViewed("z.ts", null, new Map([["z.ts", true]]))).toBe(true);
  });
});

describe("countViewedFiles", () => {
  it("counts only the files on screen, presses included", () => {
    expect(countViewedFiles(["a.ts", "b.ts", "c.ts"], states, NO_OVERLAY)).toBe(1);
    expect(countViewedFiles(["a.ts", "b.ts", "c.ts"], states, new Map([["b.ts", true]]))).toBe(2);
    // A file the host knows about but the diff has not paged in yet is not counted.
    expect(countViewedFiles(["b.ts"], states, NO_OVERLAY)).toBe(0);
  });
});

describe("settleFileViewedOverlay", () => {
  it("drops a press the host has caught up on", () => {
    const settled = settleFileViewedOverlay(new Map([["a.ts", true]]), states, NOTHING_PENDING);
    expect(settled.size).toBe(0);
  });

  it("keeps a press the host still disagrees with", () => {
    const overlay = new Map([["b.ts", true]]);
    expect(settleFileViewedOverlay(overlay, states, NOTHING_PENDING)).toBe(overlay);
  });

  it("keeps a press the host cannot have heard yet", () => {
    // An answer already on its way when the file was un-ticked would otherwise put the tick back.
    const overlay = new Map([["a.ts", false]]);
    const settled = settleFileViewedOverlay(overlay, states, new Set(["a.ts"]));
    expect(settled.get("a.ts")).toBe(false);
  });

  it("settles a file pushed to since it was cleared against un-ticking it", () => {
    const settled = settleFileViewedOverlay(new Map([["c.ts", false]]), states, NOTHING_PENDING);
    expect(settled.size).toBe(0);
  });

  it("holds everything until the host has answered at all", () => {
    const overlay = new Map([["a.ts", true]]);
    expect(settleFileViewedOverlay(overlay, null, NOTHING_PENDING)).toBe(overlay);
  });
});

describe("toFileViewedBatch", () => {
  it("carries both directions in one batch", () => {
    expect(
      toFileViewedBatch(
        new Map([
          ["a.ts", false],
          ["b.ts", true],
        ]),
      ),
    ).toEqual([
      { path: "a.ts", viewed: false },
      { path: "b.ts", viewed: true },
    ]);
  });
});
