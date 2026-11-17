import type { Element, ElementContent } from "hast";

/** Widest a column's floor gets, in characters; longer cells wrap above it. */
export const MARKDOWN_TABLE_COLUMN_FLOOR_CH = 16;

function textContent(node: ElementContent): string {
  if (node.type === "text") return node.value;
  if (node.type !== "element") return "";
  let text = "";
  for (const child of node.children) text += textContent(child);
  return text;
}

function* tableRows(node: Element): Generator<Element> {
  for (const child of node.children) {
    if (child.type !== "element") continue;
    if (child.tagName === "tr") yield child;
    else if (child.tagName === "thead" || child.tagName === "tbody" || child.tagName === "tfoot") {
      yield* tableRows(child);
    }
  }
}

/**
 * The narrowest a table can get before its cells turn into tall, hard-to-scan
 * blocks: each column keeps room for its longest cell, up to the floor. Narrow
 * columns such as numbers keep their natural width. Returns the summed floor in
 * `ch` and the column count, or null when the table has no cells.
 */
export function markdownTableMinWidth(
  table: Element | undefined,
): { readonly ch: number; readonly columns: number } | null {
  if (!table) return null;
  const longest: number[] = [];
  for (const row of tableRows(table)) {
    let column = 0;
    for (const cell of row.children) {
      if (cell.type !== "element" || (cell.tagName !== "th" && cell.tagName !== "td")) continue;
      longest[column] = Math.max(longest[column] ?? 0, textContent(cell).trim().length);
      column += 1;
    }
  }
  if (longest.length === 0) return null;
  let ch = 0;
  for (const length of longest) ch += Math.min(length, MARKDOWN_TABLE_COLUMN_FLOOR_CH);
  return { ch, columns: longest.length };
}
