import {
  COMPOSER_MENTION_DRAG_TYPE,
  composerMentionFromTreePath,
} from "~/components/chat/composerMentionDrag";

interface FileTreeDragTransfer {
  setData(format: string, data: string): void;
}

export interface FileTreeDragStartEvent {
  readonly dataTransfer: FileTreeDragTransfer | null;
  composedPath(): ReadonlyArray<unknown>;
}

export interface FileTreeDragMentionHost {
  /** Drop the tree's gesture-applied selection of the dragged row. */
  deselect(treePath: string): void;
}

export interface FileTreeDragMentionController {
  /**
   * True from the moment a row drag starts until it ends. The tree selects
   * the dragged row as part of the gesture; selection changes made while
   * this is set are gesture side effects, not requests to open a file.
   */
  isDragInProgress(): boolean;
  handleDragStart(event: FileTreeDragStartEvent): void;
  handleDragEnd(): void;
}

const itemPathOf = (node: unknown): string | null => {
  if (typeof node !== "object" || node === null) {
    return null;
  }
  const element = node as { getAttribute?: (name: string) => string | null };
  return typeof element.getAttribute === "function" ? element.getAttribute("data-item-path") : null;
};

/**
 * Tags file-tree drags with the composer mention payload and keeps the drag
 * from acting like a click: while the drag runs, selection changes are
 * suppressed, and when it ends the dragged row is deselected so it is not
 * left highlighted and a later click on it still fires a selection change.
 */
export function createFileTreeDragMentionController(
  host: FileTreeDragMentionHost,
): FileTreeDragMentionController {
  let draggedPath: string | null = null;
  return {
    isDragInProgress: () => draggedPath !== null,
    handleDragStart(event) {
      if (event.dataTransfer === null) {
        return;
      }
      // Only drags that originate on a tree row are mentions; a text/plain
      // fallback would also tag drags of selected text from the panel chrome.
      let itemPath: string | null = null;
      for (const node of event.composedPath()) {
        itemPath = itemPathOf(node);
        if (itemPath !== null) {
          break;
        }
      }
      const mention = composerMentionFromTreePath(itemPath ?? "");
      if (itemPath === null || mention === null) {
        return;
      }
      draggedPath = itemPath;
      event.dataTransfer.setData(COMPOSER_MENTION_DRAG_TYPE, mention);
    },
    handleDragEnd() {
      if (draggedPath === null) {
        return;
      }
      host.deselect(draggedPath);
      draggedPath = null;
    },
  };
}
