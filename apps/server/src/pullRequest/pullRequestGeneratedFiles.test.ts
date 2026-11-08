import { describe, expect, it } from "vitest";

import {
  gitattributesDirectories,
  isGeneratedPath,
  parseGitattributes,
} from "./pullRequestGeneratedFiles.ts";

const resolve = (files: ReadonlyArray<readonly [string, string]>, path: string) =>
  isGeneratedPath(
    files.map(([directory, text]) => parseGitattributes(directory, text)),
    path,
  );

describe("isGeneratedPath", () => {
  it("treats set and =true as generated and -attr, =false as not", () => {
    const root = [
      [
        "",
        [
          "*.pb.go linguist-generated",
          "dist/** linguist-generated=true",
          "dist/keep.js -linguist-generated",
          "other.gen linguist-generated=false",
          "# comment linguist-generated",
        ].join("\n"),
      ],
    ] as const;
    expect(resolve(root, "api/x.pb.go")).toBe(true);
    expect(resolve(root, "dist/a/b.js")).toBe(true);
    expect(resolve(root, "dist/keep.js")).toBe(false);
    expect(resolve(root, "other.gen")).toBe(false);
    expect(resolve(root, "src/main.ts")).toBe(false);
  });

  it("anchors patterns with a slash and lets a nested file override the root", () => {
    const files = [
      ["", "docs/*.md linguist-generated\n*.snap linguist-generated"],
      ["pkg", "*.snap -linguist-generated\n/out.txt linguist-generated"],
    ] as const;
    expect(resolve(files, "docs/a.md")).toBe(true);
    expect(resolve(files, "other/docs/a.md")).toBe(false);
    expect(resolve(files, "a/b.snap")).toBe(true);
    expect(resolve(files, "pkg/a.snap")).toBe(false);
    expect(resolve(files, "pkg/out.txt")).toBe(true);
    expect(resolve(files, "out.txt")).toBe(false);
  });

  it("lets the last matching line win and skips directory-only patterns", () => {
    const files = [
      ["", "gen/ linguist-generated\n*.js linguist-generated\nx.js -linguist-generated"],
    ] as const;
    expect(resolve(files, "gen/a.ts")).toBe(false);
    expect(resolve(files, "a.js")).toBe(true);
    expect(resolve(files, "x.js")).toBe(false);
  });
});

describe("gitattributesDirectories", () => {
  it("returns the root and ancestors shallowest first within the limit", () => {
    expect(gitattributesDirectories(["a/b/c.ts", "a/d.ts", "e.ts"], 10)).toEqual(["", "a", "a/b"]);
    expect(gitattributesDirectories(["a/b/c/d.ts"], 2)).toEqual(["", "a"]);
  });
});
