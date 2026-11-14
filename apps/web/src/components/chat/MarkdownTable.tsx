import { CheckIcon, CopyIcon, Maximize2Icon } from "lucide-react";
import { useRef, useState, type ComponentProps, type CSSProperties } from "react";

import { useCopyToClipboard } from "../../hooks/useCopyToClipboard";
import {
  serializeTableElementToCsv,
  serializeTableElementToMarkdown,
} from "../../markdown-clipboard";
import { Button } from "../ui/button";
import { Dialog, DialogPopup, DialogTitle } from "../ui/dialog";
import { Menu, MenuItem, MenuPopup, MenuTrigger } from "../ui/menu";
import { ScrollArea } from "../ui/scroll-area";
import { Tooltip, TooltipPopup, TooltipTrigger } from "../ui/tooltip";
import { reportMarkdownActionFailure } from "./markdownActionFailure";
import type { markdownTableMinWidth } from "./markdownTableLayout";

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
  const { copyToClipboard, isCopied: copied } = useCopyToClipboard<"markdown" | "csv">({
    timeout: 1200,
    onError: (cause, format) =>
      reportMarkdownActionFailure({ operation: "copy-table", format }, cause),
  });
  const copyLabel = copied ? "Copied" : "Copy table";

  const handleCopy = (format: "markdown" | "csv") => {
    const table = getTable();
    if (!table) return;
    copyToClipboard(
      format === "markdown"
        ? serializeTableElementToMarkdown(table)
        : serializeTableElementToCsv(table),
      format,
    );
  };

  const icon = copied ? <CheckIcon className="size-3" /> : <CopyIcon className="size-3" />;
  return (
    <Menu>
      {labeled ? (
        <MenuTrigger render={<Button type="button" variant="ghost-muted" size="xs" />}>
          {icon}
          {copied ? "Copied" : "Copy"}
        </MenuTrigger>
      ) : (
        <Tooltip>
          <TooltipTrigger
            render={
              <MenuTrigger
                render={
                  <Button
                    type="button"
                    variant="ghost-muted"
                    size="icon-xs"
                    aria-label={copyLabel}
                  />
                }
              />
            }
          >
            {icon}
          </TooltipTrigger>
          <TooltipPopup side="top">{copyLabel}</TooltipPopup>
        </Tooltip>
      )}
      <MenuPopup align="end">
        <MenuItem onClick={() => handleCopy("markdown")}>Copy as Markdown</MenuItem>
        <MenuItem onClick={() => handleCopy("csv")}>Copy as CSV</MenuItem>
      </MenuPopup>
    </Menu>
  );
}

export function MarkdownTable({ children, minWidth, style, ...props }: MarkdownTableProps) {
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
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  type="button"
                  variant="ghost-muted"
                  size="icon-xs"
                  aria-label="Expand table"
                  onClick={() => setExpanded(true)}
                />
              }
            >
              <Maximize2Icon className="size-3" />
            </TooltipTrigger>
            <TooltipPopup side="top">Expand table</TooltipPopup>
          </Tooltip>
        </span>
        <TableCopyMenu getTable={() => containerRef.current?.querySelector("table")} />
      </div>
      <Dialog open={expanded} onOpenChange={setExpanded}>
        <DialogPopup className="h-full max-w-none sm:max-w-[min(96vw,120rem)]">
          <div className="flex items-center gap-2 border-b py-2 ps-4 pe-12">
            <DialogTitle>Table</DialogTitle>
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
