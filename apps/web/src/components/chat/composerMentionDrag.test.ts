import { describe, expect, it } from "@effect/vitest";

import {
  COMPOSER_MENTION_DRAG_TYPE,
  composerMentionFromTreePath,
  dataTransferHasComposerMention,
} from "./composerMentionDrag.ts";

describe("composerMentionFromTreePath", () => {
  it("serializes a file path into a mention", () => {
    expect(composerMentionFromTreePath("docs/index.md")).toBe("[index.md](docs/index.md)");
  });

  it("strips the trailing slash directory rows carry", () => {
    expect(composerMentionFromTreePath("docs/architecture/")).toBe(
      "[architecture](docs/architecture)",
    );
  });

  it("rejects drags that carry no path", () => {
    expect(composerMentionFromTreePath("")).toBeNull();
    expect(composerMentionFromTreePath("/")).toBeNull();
  });
});

describe("dataTransferHasComposerMention", () => {
  it("detects the mention payload among drag types", () => {
    expect(dataTransferHasComposerMention([COMPOSER_MENTION_DRAG_TYPE, "text/plain"])).toBe(true);
    expect(dataTransferHasComposerMention(["Files"])).toBe(false);
    expect(dataTransferHasComposerMention([])).toBe(false);
  });
});
