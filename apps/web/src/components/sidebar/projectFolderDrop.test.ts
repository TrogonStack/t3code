import { describe, expect, it, vi } from "@effect/vitest";
import {
  makeProjectFolderDropHandlers,
  type ProjectFolderDragEvent,
  type ProjectFolderDropHost,
  type ProjectFolderDropItem,
} from "./projectFolderDrop";

function makeItem(options: { name: string; isDirectory: boolean; asFile?: boolean }) {
  return {
    webkitGetAsEntry: () => ({ isDirectory: options.isDirectory }),
    getAsFile: () =>
      options.asFile === false ? null : { name: options.name, size: options.isDirectory ? 0 : 12 },
  } satisfies ProjectFolderDropItem;
}

function makeDragEvent(options?: {
  types?: string[];
  items?: ProjectFolderDropItem[];
  movedWithinTarget?: boolean;
}) {
  const preventDefault = vi.fn();
  const event = {
    dataTransfer: {
      types: options?.types ?? ["Files"],
      items: options?.items ?? [],
      dropEffect: "none",
    },
    relatedTarget: options?.movedWithinTarget ? ({} as EventTarget) : null,
    currentTarget: {
      contains: () => options?.movedWithinTarget ?? false,
    },
    preventDefault,
  } satisfies ProjectFolderDragEvent;
  return { event, preventDefault };
}

function makeHost(options?: { resolvedPath?: string | null }) {
  const setDragActive = vi.fn();
  const addProjectAtPath = vi.fn();
  const rejectDrop = vi.fn();
  const resolvedPath = options && "resolvedPath" in options ? options.resolvedPath : "/repos/api";
  const resolveDroppedFolderPath = vi.fn(() => resolvedPath ?? null);
  const host = {
    setDragActive,
    addProjectAtPath,
    rejectDrop,
    resolveDroppedFolderPath,
  } satisfies ProjectFolderDropHost;
  return { host, setDragActive, addProjectAtPath, rejectDrop, resolveDroppedFolderPath };
}

describe("makeProjectFolderDropHandlers", () => {
  it("activates the target for an external file drag", () => {
    const { host, setDragActive } = makeHost();
    const { event, preventDefault } = makeDragEvent();

    makeProjectFolderDropHandlers(host).onDragEnter(event);

    expect(preventDefault).toHaveBeenCalledOnce();
    expect(setDragActive).toHaveBeenCalledWith(true);
  });

  it("ignores drags that carry no files, such as sidebar thread reordering", () => {
    const { host, setDragActive } = makeHost();
    const { event, preventDefault } = makeDragEvent({ types: ["text/plain"] });

    makeProjectFolderDropHandlers(host).onDragOver(event);

    expect(preventDefault).not.toHaveBeenCalled();
    expect(setDragActive).not.toHaveBeenCalled();
  });

  it("does not flicker when the drag moves between children", () => {
    const { host, setDragActive } = makeHost();
    const { event } = makeDragEvent({ movedWithinTarget: true });

    const handlers = makeProjectFolderDropHandlers(host);
    handlers.onDragEnter(event);
    handlers.onDragLeave(event);

    expect(setDragActive).not.toHaveBeenCalled();
  });

  it("adds the first dropped folder and clears the active state", () => {
    const { host, setDragActive, addProjectAtPath, rejectDrop } = makeHost();
    const { event } = makeDragEvent({
      items: [
        makeItem({ name: "notes.md", isDirectory: false }),
        makeItem({ name: "api", isDirectory: true }),
        makeItem({ name: "web", isDirectory: true }),
      ],
    });

    makeProjectFolderDropHandlers(host).onDrop(event);

    expect(setDragActive).toHaveBeenCalledWith(false);
    expect(addProjectAtPath).toHaveBeenCalledWith("/repos/api");
    expect(rejectDrop).not.toHaveBeenCalled();
  });

  it("rejects a drop that carries only files", () => {
    const { host, addProjectAtPath, rejectDrop, resolveDroppedFolderPath } = makeHost();
    const { event } = makeDragEvent({
      items: [makeItem({ name: "notes.md", isDirectory: false })],
    });

    makeProjectFolderDropHandlers(host).onDrop(event);

    expect(rejectDrop).toHaveBeenCalledWith("no-folder");
    expect(resolveDroppedFolderPath).not.toHaveBeenCalled();
    expect(addProjectAtPath).not.toHaveBeenCalled();
  });

  it("rejects a folder whose path cannot be resolved", () => {
    const { host, addProjectAtPath, rejectDrop } = makeHost({ resolvedPath: null });
    const { event } = makeDragEvent({ items: [makeItem({ name: "api", isDirectory: true })] });

    makeProjectFolderDropHandlers(host).onDrop(event);

    expect(rejectDrop).toHaveBeenCalledWith("path-unresolved");
    expect(addProjectAtPath).not.toHaveBeenCalled();
  });

  it("skips a folder entry that no longer exposes a file", () => {
    const { host, addProjectAtPath, rejectDrop } = makeHost();
    const { event } = makeDragEvent({
      items: [makeItem({ name: "api", isDirectory: true, asFile: false })],
    });

    makeProjectFolderDropHandlers(host).onDrop(event);

    expect(rejectDrop).toHaveBeenCalledWith("no-folder");
    expect(addProjectAtPath).not.toHaveBeenCalled();
  });
});
