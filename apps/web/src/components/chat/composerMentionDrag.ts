import { serializeComposerFileLink } from "@t3tools/shared/composerTrigger";

/**
 * Drag payload type carrying a serialized composer mention. Set on drags that
 * start in the workspace file tree so the composer can tell them apart from
 * OS file drags and plain text selections.
 */
export const COMPOSER_MENTION_DRAG_TYPE = "application/x-t3code-composer-mention";

export function composerMentionFromTreePath(treePath: string): string | null {
  const relativePath = treePath.replace(/\/+$/, "");
  if (relativePath.length === 0) {
    return null;
  }
  return serializeComposerFileLink(relativePath);
}

export function dataTransferHasComposerMention(types: ReadonlyArray<string>): boolean {
  return types.includes(COMPOSER_MENTION_DRAG_TYPE);
}
