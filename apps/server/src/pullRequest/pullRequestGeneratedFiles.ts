interface AttributeRule {
  readonly pattern: RegExp;
  readonly generated: boolean;
}

export interface GitattributesRules {
  readonly directory: string;
  readonly rules: ReadonlyArray<AttributeRule>;
}

const ATTRIBUTE = "linguist-generated";
const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&");

function globSource(glob: string): string {
  let source = "";
  for (let at = 0; at < glob.length; at += 1) {
    const char = glob.charAt(at);
    if (char === "*") {
      const leading = at === 0 || glob.charAt(at - 1) === "/";
      if (glob.charAt(at + 1) === "*" && leading && glob.charAt(at + 2) === "/") {
        source += "(?:.*/)?";
        at += 2;
      } else if (glob.charAt(at + 1) === "*" && leading && at + 2 === glob.length) {
        source += ".*";
        at += 1;
      } else {
        source += "[^/]*";
      }
    } else if (char === "?") {
      source += "[^/]";
    } else if (char === "[" && glob.indexOf("]", at + 2) !== -1) {
      const end = glob.indexOf("]", at + 2);
      const body = glob.slice(at + 1, end).replace(/\\/g, "\\\\");
      source += `[${body.startsWith("!") ? `^${body.slice(1)}` : body}]`;
      at = end;
    } else if (char === "\\" && at + 1 < glob.length) {
      at += 1;
      source += escapeRegExp(glob.charAt(at));
    } else {
      source += escapeRegExp(char);
    }
  }
  return source;
}

function patternOf(token: string): RegExp | null {
  if (token.startsWith('"') || token.startsWith("[attr]") || token.endsWith("/")) return null;
  const glob = token.startsWith("/") ? token.slice(1) : token;
  if (glob === "") return null;
  try {
    return new RegExp(`^${token.includes("/") ? "" : "(?:.*/)?"}${globSource(glob)}$`);
  } catch {
    return null;
  }
}

/** Keeps only linguist-generated decisions; unspecifying it counts as not generated. */
export function parseGitattributes(directory: string, text: string): GitattributesRules {
  const rules: Array<AttributeRule> = [];
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) continue;
    const [token, ...attributes] = line.split(/\s+/);
    if (token === undefined) continue;
    let generated: boolean | null = null;
    for (const attribute of attributes) {
      if (attribute === ATTRIBUTE || attribute === `${ATTRIBUTE}=true`) generated = true;
      else if (
        attribute === `-${ATTRIBUTE}` ||
        attribute === `!${ATTRIBUTE}` ||
        attribute === `${ATTRIBUTE}=false`
      ) {
        generated = false;
      }
    }
    if (generated === null) continue;
    const pattern = patternOf(token);
    if (pattern !== null) rules.push({ pattern, generated });
  }
  return { directory, rules };
}

function stateWithin(file: GitattributesRules, path: string): boolean | null {
  const prefix = file.directory === "" ? "" : `${file.directory}/`;
  if (!path.startsWith(prefix)) return null;
  const relative = path.slice(prefix.length);
  const last = file.rules.findLast((rule) => rule.pattern.test(relative));
  return last === undefined ? null : last.generated;
}

/** The deepest .gitattributes with an opinion wins, and the last matching line within it. */
export function isGeneratedPath(files: ReadonlyArray<GitattributesRules>, path: string): boolean {
  const deepestFirst = files.toSorted((a, b) => b.directory.length - a.directory.length);
  for (const file of deepestFirst) {
    const state = stateWithin(file, path);
    if (state !== null) return state;
  }
  return false;
}

/** The root and every ancestor directory of the paths, shallowest first, up to `limit`. */
export function gitattributesDirectories(paths: ReadonlyArray<string>, limit: number): string[] {
  const directories = new Set([""]);
  for (const path of paths) {
    const parts = path.split("/").slice(0, -1);
    for (let depth = 1; depth <= parts.length; depth += 1) {
      directories.add(parts.slice(0, depth).join("/"));
    }
  }
  return [...directories]
    .toSorted((a, b) => (a === "" ? 0 : a.split("/").length) - (b === "" ? 0 : b.split("/").length))
    .slice(0, limit);
}
