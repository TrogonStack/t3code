/**
 * Dropping a folder onto the sidebar adds it as a project. Only the desktop
 * shell can turn a dropped folder into an absolute path, so the host decides
 * whether resolution is possible and what to do with the result.
 */
export interface ProjectFolderDropItem {
  webkitGetAsEntry(): { readonly isDirectory: boolean } | null;
  getAsFile(): { readonly name: string; readonly size: number } | null;
}

export interface ProjectFolderDragEvent {
  readonly dataTransfer: {
    readonly types: ReadonlyArray<string>;
    readonly items: ArrayLike<ProjectFolderDropItem>;
    dropEffect: string;
  };
  readonly relatedTarget: EventTarget | null;
  readonly currentTarget: {
    contains(target: Node | null): boolean;
  };
  preventDefault(): void;
}

/**
 * `no-folder`: the drop carried files but no directory.
 * `path-unresolved`: a directory was dropped but its path came back empty.
 */
export type ProjectFolderDropRejection = "no-folder" | "path-unresolved";

export interface ProjectFolderDropHost {
  setDragActive(active: boolean): void;
  resolveDroppedFolderPath(file: { readonly name: string; readonly size: number }): string | null;
  addProjectAtPath(path: string): void;
  rejectDrop(reason: ProjectFolderDropRejection): void;
}

function isFileDrag(event: ProjectFolderDragEvent): boolean {
  return event.dataTransfer.types.includes("Files");
}

function movedWithinDropTarget(event: ProjectFolderDragEvent): boolean {
  return event.relatedTarget !== null && event.currentTarget.contains(event.relatedTarget as Node);
}

/**
 * The first dropped directory, or null. Directories cannot be told apart from
 * files until the drop lands, so this runs there rather than on drag over.
 */
function findDroppedFolder(
  items: ArrayLike<ProjectFolderDropItem>,
): { readonly name: string; readonly size: number } | null {
  for (let index = 0; index < items.length; index += 1) {
    const item = items[index];
    if (!item || item.webkitGetAsEntry()?.isDirectory !== true) continue;
    const file = item.getAsFile();
    if (file) return file;
  }
  return null;
}

/**
 * Handlers for the sidebar's project area. Wire them only when the host can
 * resolve dropped paths: a highlighted drop target that can never succeed
 * reads as a bug.
 */
export function makeProjectFolderDropHandlers(host: ProjectFolderDropHost) {
  return {
    onDragEnter(event: ProjectFolderDragEvent) {
      if (!isFileDrag(event)) return;
      event.preventDefault();
      if (movedWithinDropTarget(event)) return;
      host.setDragActive(true);
    },
    onDragOver(event: ProjectFolderDragEvent) {
      if (!isFileDrag(event)) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
      host.setDragActive(true);
    },
    onDragLeave(event: ProjectFolderDragEvent) {
      if (!isFileDrag(event)) return;
      event.preventDefault();
      if (movedWithinDropTarget(event)) return;
      host.setDragActive(false);
    },
    // Several folders at once still resolve to one project, because the add
    // project surface confirms a single path.
    onDrop(event: ProjectFolderDragEvent) {
      if (!isFileDrag(event)) return;
      event.preventDefault();
      host.setDragActive(false);
      const folder = findDroppedFolder(event.dataTransfer.items);
      if (!folder) {
        host.rejectDrop("no-folder");
        return;
      }
      const path = host.resolveDroppedFolderPath(folder);
      if (!path) {
        host.rejectDrop("path-unresolved");
        return;
      }
      host.addProjectAtPath(path);
    },
  };
}
