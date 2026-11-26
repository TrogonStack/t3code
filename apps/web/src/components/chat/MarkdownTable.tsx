import { T, useGT } from "gt-react";
import { CheckIcon, CopyIcon, Maximize2Icon } from "lucide-react";
import { useRef, useState, type ComponentProps, type CSSProperties } from "react";

import { useCopyToClipboard } from "../../hooks/useCopyToClipboard";
import {
  serializeTableElementToCsv,
  serializeTableElementToHtml,
  serializeTableElementToMarkdown,
} from "../../markdown-clipboard";
import { Button } from "../ui/button";
import { Dialog, DialogPopup, DialogTitle } from "../ui/dialog";
import { Menu, MenuItem, MenuPopup, MenuTrigger } from "../ui/menu";
import { ScrollArea } from "../ui/scroll-area";
import { reportMarkdownActionFailure, type TableCopyFormat } from "./markdownActionFailure";
import { SharedTooltipTrigger } from "../ui/shared-tooltip";
import type { markdownTableMinWidth } from "./markdownTableLayout";

const TABLE_COPY_SERIALIZERS: Record<TableCopyFormat, (table: Element) => string> = {
  markdown: serializeTableElementToMarkdown,
  csv: serializeTableElementToCsv,
  html: serializeTableElementToHtml,
};

type MarkdownTableProps = ComponentProps<"table"> & {
  minWidth?: ReturnType<typeof markdownTableMinWidth>;
};

function TableCopyMenu({
  getTable,
  labeled = false,
}: {
  getTable: () => HTMLTableElement | null | undefined;
  /** A text button instead of an icon. Inside the expanded dialog a tooltip would take one Escape press of its own. */
  labeled?: boolean;
}) {
  const gt = useGT();
  const { copyToClipboard, isCopied: copied } = useCopyToClipboard<TableCopyFormat>({
    timeout: 1200,
    onError: (cause, format) =>
      reportMarkdownActionFailure({ operation: "copy-table", format }, cause),
  });
  const copyLabel = copied ? gt("Copied") : gt("Copy table");

  const handleCopy = (format: TableCopyFormat) => {
    const table = getTable();
    if (!table) return;
    const text = TABLE_COPY_SERIALIZERS[format](table);
    // Plain-text targets get the markup; rich targets (mail, docs, wikis) paste a real table.
    copyToClipboard(
      text,
      format,
      format === "html" ? { "text/html": `<meta charset="utf-8">${text}` } : undefined,
      { requireHtml: format === "html" },
    );
  };

  const icon = copied ? <CheckIcon className="size-3" /> : <CopyIcon className="size-3" />;
  return (
    <Menu>
      {labeled ? (
        <MenuTrigger render={<Button type="button" variant="ghost-muted" size="xs" />}>
          {icon}
          {copied ? <T>Copied</T> : <T>Copy</T>}
        </MenuTrigger>
      ) : (
        <SharedTooltipTrigger
          render={
            <MenuTrigger
              render={
                <Button type="button" variant="ghost-muted" size="icon-xs" aria-label={copyLabel} />
              }
            />
          }
          label={copyLabel}
        >
          {icon}
        </SharedTooltipTrigger>
      )}
      <MenuPopup align="end">
        <MenuItem onClick={() => handleCopy("markdown")}>
          <T>Copy as Markdown</T>
        </MenuItem>
        <MenuItem onClick={() => handleCopy("csv")}>
          <T>Copy as CSV</T>
        </MenuItem>
        <MenuItem onClick={() => handleCopy("html")}>
          <T>Copy as HTML</T>
        </MenuItem>
      </MenuPopup>
    </Menu>
  );
}

export function MarkdownTable({ children, minWidth, style, ...props }: MarkdownTableProps) {
  const gt = useGT();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const expandedRef = useRef<HTMLDivElement | null>(null);
  const [expanded, setExpanded] = useState(false);
  const tableStyle = minWidth
    ? ({
        ...style,
        "--markdown-table-floor": `calc(${minWidth.ch}ch + ${minWidth.columns - 1} * 1.5rem)`,
      } as CSSProperties)
    : style;

  return (
    <div ref={containerRef} className="chat-markdown-table-container">
      <div className="chat-markdown-table-breakout">
        <ScrollArea radius="none" chainVerticalScroll scrollFade className="w-full max-w-full">
          <table {...props} style={tableStyle}>
            {children}
          </table>
        </ScrollArea>
      </div>
      {/* Stays within the row's px-1: the chat lane clips sideways overflow. */}
      <div className="mt-0.5 -mx-1 flex items-center justify-end gap-0.5 select-none">
        <span className="chat-markdown-table-expand flex">
          <SharedTooltipTrigger
            render={
              <Button
                type="button"
                variant="ghost-muted"
                size="icon-xs"
                aria-label={gt("Expand table")}
                onClick={() => setExpanded(true)}
              />
            }
            label={<T>Expand table</T>}
          >
            <Maximize2Icon className="size-3" />
          </SharedTooltipTrigger>
        </span>
        <TableCopyMenu getTable={() => containerRef.current?.querySelector("table")} />
      </div>
      <Dialog open={expanded} onOpenChange={setExpanded}>
        <DialogPopup className="h-full max-w-none sm:max-w-[min(96vw,120rem)]">
          <div className="flex items-center gap-2 border-b py-2 ps-4 pe-12">
            <DialogTitle>
              <T>Table</T>
            </DialogTitle>
            <div className="ms-auto">
              <TableCopyMenu labeled getTable={() => expandedRef.current?.querySelector("table")} />
            </div>
          </div>
          <div
            ref={expandedRef}
            className="chat-markdown chat-markdown-table-expanded min-h-0 flex-1 overflow-auto pb-4"
          >
            <table {...props} style={tableStyle}>
              {children}
            </table>
          </div>
        </DialogPopup>
      </Dialog>
    </div>
  );
}
