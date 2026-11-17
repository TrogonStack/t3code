import type { Element, Root } from "hast";
import remarkGfm from "remark-gfm";
import remarkParse from "remark-parse";
import remarkRehype from "remark-rehype";
import { unified } from "unified";
import { describe, expect, it } from "vite-plus/test";

import { MARKDOWN_TABLE_COLUMN_FLOOR_CH, markdownTableMinWidth } from "./markdownTableLayout";

const processor = unified().use(remarkParse).use(remarkGfm).use(remarkRehype);

function tableFrom(markdown: string): Element {
  const root = processor.runSync(processor.parse(markdown)) as Root;
  const table = root.children.find(
    (node): node is Element => node.type === "element" && node.tagName === "table",
  );
  if (!table) throw new Error("no table");
  return table;
}

describe("markdownTableMinWidth", () => {
  it("keeps short columns at their longest cell and caps long ones at the floor", () => {
    const table = tableFrom(
      [
        "| # | Approach | Notes |",
        "| - | --- | --- |",
        "| 1 | Breakout | Grows past the reading column into the free pane width |",
        "| 12 | Scroll | **Keeps** the column |",
      ].join("\n"),
    );
    expect(markdownTableMinWidth(table)).toEqual({
      ch: 2 + "Breakout".length + MARKDOWN_TABLE_COLUMN_FLOOR_CH,
      columns: 3,
    });
    const spaced = tableFrom(["| A |", "| - |", "| **a** **b** **c** **d** |"].join("\n"));
    expect(markdownTableMinWidth(spaced)).toEqual({ ch: "a b c d".length, columns: 1 });
  });

  it("sums a floor per column, so many long columns outgrow a narrow lane", () => {
    const cell = "a long explanation that would otherwise wrap into a tall block";
    const row = `| ${Array.from({ length: 7 }, () => cell).join(" | ")} |`;
    const table = tableFrom(
      [
        `| ${Array.from({ length: 7 }, (_, index) => `Column ${index}`).join(" | ")} |`,
        `| ${Array.from({ length: 7 }, () => "---").join(" | ")} |`,
        row,
      ].join("\n"),
    );
    expect(markdownTableMinWidth(table)).toEqual({
      ch: 7 * MARKDOWN_TABLE_COLUMN_FLOOR_CH,
      columns: 7,
    });
  });

  it("returns null without a table", () => {
    expect(markdownTableMinWidth(undefined)).toBeNull();
  });
});
