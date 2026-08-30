const ENTRY = "diff --git ";

interface Entry {
  oldPath: string | null;
  newPath: string | null;
  deleted: boolean;
  revision: string | null;
  /** Past the first hunk header every line is content, and content can start like a header. */
  inBody: boolean;
}

/** `a/x` and `b/x` on a `---` or `+++` line; `/dev/null` is the side that has no file. */
function sidePath(rest: string, prefix: string): string | null {
  if (rest === "/dev/null") return null;
  return rest.startsWith(prefix) ? rest.slice(prefix.length) : rest;
}

/**
 * The two names on a `diff --git` line, which git writes with no delimiter between them.
 *
 * `a/one two b/one two` splits in more than one place, so the split that leaves both sides equal
 * wins. A rename is the only entry whose sides differ, and a rename states its names on lines of
 * its own. Anything still ambiguous is left unnamed rather than guessed at.
 */
function headerPaths(rest: string): readonly [string | null, string | null] {
  if (!rest.startsWith("a/")) return [null, null];
  const splits: Array<number> = [];
  for (let at = rest.indexOf(" b/"); at !== -1; at = rest.indexOf(" b/", at + 1)) splits.push(at);
  const chosen =
    splits.find((at) => rest.slice(2, at) === rest.slice(at + 3)) ??
    (splits.length === 1 ? splits[0] : undefined);
  return chosen === undefined ? [null, null] : [rest.slice(2, chosen), rest.slice(chosen + 3)];
}

/** The right-hand id of `index <before>..<after> <mode>`. */
function headRevision(rest: string): string | null {
  const gap = rest.indexOf("..");
  if (gap === -1) return null;
  const after = rest.slice(gap + 2);
  const end = after.indexOf(" ");
  const head = end === -1 ? after : after.slice(0, end);
  return head.length === 0 ? null : head;
}

/**
 * What the head has of each file in a unified patch, as the blob ids git writes into it.
 *
 * Bitbucket states a file's version nowhere else: its diffstat entries carry a commit and a path
 * and no blob id, and no endpoint answers what a file is now. Git's own `index <before>..<after>`
 * line is in the patch the diff already reads, so the versions cost no call of their own.
 *
 * Keyed the way the client names files: the head's name for it, except for a deletion, where the
 * head has no name and the one it had is what is on screen. An entry the patch gives no `index`
 * line for, one Bitbucket excluded by pattern most often, is left out. Left out reads the same
 * way when a file is ticked and when the tick is read back, so the mark still holds.
 */
export function parseDiffFileRevisions(patch: string): ReadonlyMap<string, string> {
  const revisions = new Map<string, string>();
  let entry: Entry | null = null;

  const close = () => {
    if (entry === null) return;
    const path = entry.deleted ? entry.oldPath : (entry.newPath ?? entry.oldPath);
    if (path !== null && path.length > 0 && entry.revision !== null) {
      revisions.set(path, entry.revision);
    }
    entry = null;
  };

  for (const line of patch.split("\n")) {
    if (line.startsWith(ENTRY)) {
      close();
      const [oldPath, newPath] = headerPaths(line.slice(ENTRY.length));
      entry = { oldPath, newPath, deleted: false, revision: null, inBody: false };
      continue;
    }
    if (entry === null || entry.inBody) continue;
    if (line.startsWith("@@")) {
      entry.inBody = true;
    } else if (line.startsWith("index ")) {
      entry.revision = headRevision(line.slice("index ".length));
    } else if (line.startsWith("deleted file mode")) {
      entry.deleted = true;
    } else if (line.startsWith("rename from ")) {
      entry.oldPath = line.slice("rename from ".length);
    } else if (line.startsWith("rename to ")) {
      entry.newPath = line.slice("rename to ".length);
    } else if (line.startsWith("--- ")) {
      entry.oldPath = sidePath(line.slice(4), "a/");
    } else if (line.startsWith("+++ ")) {
      const side = sidePath(line.slice(4), "b/");
      entry.newPath = side;
      if (side === null) entry.deleted = true;
    }
  }
  close();
  return revisions;
}
