// @effect-diagnostics nodeBuiltinImport:off - a codegen script that reads and writes one file.
// Writes schema/rpc.schema.json: every WsRpcGroup method's payload, success and error schema as
// one JSON Schema document with shared $defs, for clients that are not TypeScript (the GPUI
// desktop generates its Rust wire types from it). It describes the encoded (wire) side.
//
//   vp run --filter @t3tools/contracts generate          rewrite the file
//   vp run --filter @t3tools/contracts generate --check  fail when the file is stale
import * as NodeFS from "node:fs";
import * as NodePath from "node:path";
import * as NodeURL from "node:url";

import * as Schema from "effect/Schema";
import { RpcSchema } from "effect/rpc";

import { WsRpcGroup } from "../src/rpc.ts";

type Json =
  | null
  | boolean
  | number
  | string
  | ReadonlyArray<Json>
  | { readonly [key: string]: Json };
type JsonObject = { [key: string]: Json };

const REF_PREFIX = "#/$defs/";
// Repeated subtrees smaller than this stay inline: a $ref would cost about as much as the copy.
const MIN_HOIST_BYTES = 200;

export const SCHEMA_PATH = NodePath.join(
  NodeURL.fileURLToPath(new URL("..", import.meta.url)),
  "schema",
  "rpc.schema.json",
);

const isObject = (value: Json): value is JsonObject =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const pascal = (text: string) =>
  text
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map((word) => word[0]!.toUpperCase() + word.slice(1))
    .join("");

const renameRefs = (node: Json, renames: ReadonlyMap<string, string>): Json => {
  if (Array.isArray(node)) return node.map((child) => renameRefs(child, renames));
  if (!isObject(node)) return node;
  const out: JsonObject = {};
  for (const [key, child] of Object.entries(node)) {
    out[key] =
      key === "$ref" && typeof child === "string" && child.startsWith(REF_PREFIX)
        ? REF_PREFIX +
          (renames.get(child.slice(REF_PREFIX.length)) ?? child.slice(REF_PREFIX.length))
        : renameRefs(child, renames);
  }
  return out;
};

interface Root {
  readonly tag: string;
  readonly part: "payload" | "success" | "error";
  readonly schema: Schema.Top;
}

const rootsOf = () => {
  const methods: Array<{ tag: string; stream: boolean; roots: ReadonlyArray<Root> }> = [];
  for (const rpc of [...WsRpcGroup.requests.values()].sort((a, b) =>
    a._tag.localeCompare(b._tag),
  )) {
    const stream = RpcSchema.isStreamSchema(rpc.successSchema);
    // A stream method's success is the element schema and its error the stream error; the rpc's
    // own error schema is Never.
    const success = RpcSchema.isStreamSchema(rpc.successSchema)
      ? rpc.successSchema.success
      : rpc.successSchema;
    const error = RpcSchema.isStreamSchema(rpc.successSchema)
      ? rpc.successSchema.error
      : rpc.errorSchema;
    methods.push({
      tag: rpc._tag,
      stream,
      roots: [
        { tag: rpc._tag, part: "payload", schema: rpc.payloadSchema as Schema.Top },
        { tag: rpc._tag, part: "success", schema: success as Schema.Top },
        { tag: rpc._tag, part: "error", schema: error as Schema.Top },
      ],
    });
  }
  return methods;
};

const NULL_SCHEMA = '{"type":"null"}';
const canon = (node: Json) => JSON.stringify(node);
const isBareRef = (node: Json) =>
  isObject(node) && typeof node.$ref === "string" && Object.keys(node).length === 1;
// Optional-and-nullable fields encode as an anyOf that repeats `null`.
const dedupeAlternatives = (node: Json): Json => {
  if (Array.isArray(node)) return node.map(dedupeAlternatives);
  if (!isObject(node)) return node;
  const out = Object.fromEntries(
    Object.entries(node).map(([key, child]) => [key, dedupeAlternatives(child)]),
  );
  if (Array.isArray(out.anyOf)) {
    const seen = new Set<string>();
    out.anyOf = out.anyOf.filter(
      (alternative) => !seen.has(canon(alternative)) && seen.add(canon(alternative)),
    );
  }
  return out;
};
const refTo = (name: string): Json => ({ $ref: REF_PREFIX + name });

export const buildRpcSchemaDocument = () => {
  const methods = rootsOf();
  const defs = new Map<string, Json>();

  // Named definitions come from identifier annotations. Two contracts can reuse one identifier
  // for different shapes, so a clash gets a numeric suffix and that schema's references follow.
  const rawShapes = new Map<string, string>();
  const trees = methods.map((method) => ({
    method,
    parts: method.roots.map((root) => {
      const document = Schema.toJsonSchemaDocument(root.schema) as unknown as {
        schema: Json;
        definitions: Record<string, Json>;
      };
      const renames = new Map<string, string>();
      const entries = Object.entries(document.definitions);
      for (const [name, value] of entries) {
        let candidate = name;
        for (
          let n = 2;
          rawShapes.has(candidate) && rawShapes.get(candidate) !== canon(value);
          n++
        ) {
          candidate = `${name}${n}`;
        }
        if (candidate !== name) renames.set(name, candidate);
        rawShapes.set(candidate, canon(value));
      }
      for (const [name, value] of entries) {
        defs.set(renames.get(name) ?? name, dedupeAlternatives(renameRefs(value, renames)));
      }
      return { root, schema: dedupeAlternatives(renameRefs(document.schema, renames)) };
    }),
  }));

  // Shapes that occur more than once across the whole forest become shared definitions, named
  // after the property they most often sit under (`delegatedFrom` -> DelegatedFrom, an array's
  // elements -> ThreadsEntry), so a name does not depend on which method came first.
  const counts = new Map<string, number>();
  const contexts = new Map<string, Map<string, number>>();
  const tally = (node: Json, context: string) => {
    if (Array.isArray(node)) return node.forEach((child) => tally(child, context));
    if (!isObject(node)) return;
    const key = canon(node);
    if (key.length >= MIN_HOIST_BYTES) {
      counts.set(key, (counts.get(key) ?? 0) + 1);
      if (context) {
        const seen = contexts.get(key) ?? new Map<string, number>();
        seen.set(context, (seen.get(context) ?? 0) + 1);
        contexts.set(key, seen);
      }
    }
    for (const [child, value] of Object.entries(node)) {
      if (child === "properties" && isObject(value)) {
        Object.entries(value).forEach(([prop, schema]) => tally(schema, pascal(prop)));
      } else {
        tally(value, child === "items" ? `${context}Entry` : context);
      }
    }
  };
  trees.forEach(({ parts }) => parts.forEach(({ schema }) => tally(schema, "")));
  defs.forEach((body, name) => tally(body, name));
  const nameFor = (key: string) =>
    [...(contexts.get(key) ?? [["Shape", 0]])].sort(
      ([a, x], [b, y]) => y - x || a.localeCompare(b),
    )[0]![0];

  const taken = new Set(defs.keys());
  const claim = (base: string) => {
    let name = base;
    for (let n = 2; taken.has(name); n++) name = `${base}${n}`;
    taken.add(name);
    return name;
  };
  const byShape = new Map<string, string>();
  const shared = new Map<string, Json>();
  const inner = new Set<string>();

  const children = (node: JsonObject): JsonObject =>
    Object.fromEntries(
      Object.entries(node).map(([key, child]) => [
        key,
        key === "properties" && isObject(child)
          ? Object.fromEntries(Object.entries(child).map(([prop, value]) => [prop, rewrite(value)]))
          : rewrite(child),
      ]),
    );
  // The outermost repeated shape wins. Its body is rewritten once, so shapes repeated only
  // inside copies of it are not hoisted separately (unused leftovers are inlined below).
  const rewrite = (node: Json): Json => {
    if (Array.isArray(node)) return node.map(rewrite);
    if (!isObject(node) || isBareRef(node)) return node;
    const key = canon(node);
    // typify names the payload of a nullable definition after the definition, so hoist only the
    // non-null shape and keep the null alternative inline.
    const nullable =
      Array.isArray(node.anyOf) &&
      node.anyOf.some((alternative) => canon(alternative) === NULL_SCHEMA);
    if (nullable || (counts.get(key) ?? 0) < 2) return children(node);
    let name = byShape.get(key);
    if (!name) {
      name = claim(nameFor(key));
      byShape.set(key, name);
      inner.add(name);
      shared.set(name, children(node));
    }
    return refTo(name);
  };

  const index: Record<string, JsonObject> = {};
  for (const { method, parts } of trees) {
    const entry: JsonObject = { stream: method.stream };
    for (const { root, schema } of parts) {
      let reference = schema;
      if (!isBareRef(schema)) {
        // The same shape under two methods (a void payload, a shared error union) shares one def.
        const key = canon(schema);
        let name = byShape.get(key);
        if (!name) {
          name = claim(`${pascal(root.tag)}${pascal(root.part)}`);
          byShape.set(key, name);
          shared.set(name, children(schema as JsonObject));
        }
        inner.delete(name);
        reference = refTo(name);
      }
      entry[root.part] = reference;
    }
    index[method.tag] = entry;
  }
  for (const [name, body] of defs) {
    defs.set(name, isObject(body) ? children(body) : body);
  }
  for (const [name, body] of shared) defs.set(name, body);

  // A hoisted inner shape used once, from a single definition, reads better inline.
  const inline = (node: Json, uses: ReadonlyMap<string, number>): Json => {
    if (Array.isArray(node)) return node.map((child) => inline(child, uses));
    if (!isObject(node)) return node;
    const ref = node.$ref;
    if (typeof ref === "string" && isBareRef(node)) {
      const name = ref.slice(REF_PREFIX.length);
      if (inner.has(name) && uses.get(name) === 1) return inline(defs.get(name)!, uses);
      return node;
    }
    return Object.fromEntries(
      Object.entries(node).map(([key, child]) => [key, inline(child, uses)]),
    );
  };
  for (;;) {
    const uses = new Map<string, number>();
    const scan = (node: Json) => {
      if (Array.isArray(node)) return node.forEach(scan);
      if (!isObject(node)) return;
      if (isBareRef(node)) {
        const name = (node.$ref as string).slice(REF_PREFIX.length);
        uses.set(name, (uses.get(name) ?? 0) + 1);
        return;
      }
      Object.values(node).forEach(scan);
    };
    defs.forEach(scan);
    Object.values(index).forEach(scan);
    const drop = [...inner].filter((name) => (uses.get(name) ?? 0) <= 1);
    if (drop.length === 0) break;
    const rewritten = new Map([...defs].map(([name, body]) => [name, inline(body, uses)] as const));
    for (const name of drop) {
      rewritten.delete(name);
      inner.delete(name);
    }
    defs.clear();
    rewritten.forEach((body, name) => defs.set(name, body));
  }

  const sorted: JsonObject = {};
  for (const name of [...defs.keys()].sort()) sorted[name] = defs.get(name)!;
  return {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    title: "T3 Code WebSocket RPC (encoded)",
    $defs: sorted,
    methods: index,
  };
};

export const renderRpcSchemaDocument = () =>
  JSON.stringify(buildRpcSchemaDocument(), null, 1) + "\n";

if (import.meta.main) {
  const next = renderRpcSchemaDocument();
  if (process.argv.includes("--check")) {
    const current = NodeFS.existsSync(SCHEMA_PATH) ? NodeFS.readFileSync(SCHEMA_PATH, "utf8") : "";
    if (current !== next) {
      console.error(
        "packages/contracts/schema/rpc.schema.json is stale. Run `vp run --filter @t3tools/contracts generate` and commit it.",
      );
      process.exitCode = 1;
    }
  } else {
    NodeFS.mkdirSync(NodePath.dirname(SCHEMA_PATH), { recursive: true });
    NodeFS.writeFileSync(SCHEMA_PATH, next);
    console.log(`wrote ${NodePath.relative(process.cwd(), SCHEMA_PATH)} (${next.length} bytes)`);
  }
}
