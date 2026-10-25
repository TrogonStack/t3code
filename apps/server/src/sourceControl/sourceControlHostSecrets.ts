import {
  isSecretProviderSecretSource,
  type SecretProviderSecretSource,
  secretSettingsFields,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";

import { sourceControlDefinitions } from "./sourceControlDefinitions.ts";

export const SOURCE_CONTROL_HOST_SECRET_FIELDS: ReadonlyMap<
  string,
  ReadonlyArray<string>
> = new Map(
  sourceControlDefinitions.definitions.flatMap((definition) => {
    const fields = definition.settings ? secretSettingsFields(definition.settings) : [];
    return fields.length > 0 ? [[definition.kind, fields] as const] : [];
  }),
);

export interface SourceControlHostSecretSlot {
  readonly kind: string;
  readonly field: string;
  readonly serverHost: string | null;
}

/** The secret store entry for one slot; names stay stable across releases. */
export function sourceControlHostSecretName(slot: SourceControlHostSecretSlot): string {
  const encode = (value: string) => Buffer.from(value, "utf8").toString("base64url");
  return slot.serverHost === null
    ? `source-control-${encode(slot.kind)}-${encode(slot.field)}`
    : `source-control-${encode(slot.kind)}-${encode(slot.field)}-${encode(slot.serverHost.trim().toLowerCase())}`;
}

export function readSecretFieldValues(
  value: unknown,
): ReadonlyArray<readonly [serverHost: string | null, value: string]> {
  if (typeof value === "string") return [[null, value]];
  if (isSecretProviderSecretSource(value)) return [];
  if (value === null || typeof value !== "object" || Array.isArray(value)) return [];
  return Object.entries(value).flatMap(([serverHost, entry]) =>
    typeof entry === "string" ? [[serverHost.trim().toLowerCase(), entry] as const] : [],
  );
}

export function writeSecretFieldValues(
  original: unknown,
  values: ReadonlyArray<readonly [serverHost: string | null, value: string]>,
): unknown {
  if (typeof original === "string") return values[0]?.[1] ?? "";
  if (isSecretProviderSecretSource(original)) return original;
  const sources =
    original !== null && typeof original === "object" && !Array.isArray(original)
      ? Object.entries(original).flatMap(([serverHost, entry]) =>
          isSecretProviderSecretSource(entry)
            ? [[serverHost.trim().toLowerCase(), entry] as const]
            : [],
        )
      : [];
  return Object.fromEntries([
    ...sources,
    ...values.map(([serverHost, value]) => [serverHost ?? "", value] as const),
  ]);
}

type SourceControlHostsSettings = Readonly<Record<string, Readonly<Record<string, unknown>>>>;

/** Replaces each secret provider reference in a credential field with the secret it names. */
export function resolveSourceControlHostSecrets<R>(
  hosts: SourceControlHostsSettings,
  resolve: (source: SecretProviderSecretSource) => Effect.Effect<string, never, R>,
): Effect.Effect<SourceControlHostsSettings, never, R> {
  const resolveField = (value: unknown): Effect.Effect<unknown, never, R> | null => {
    if (isSecretProviderSecretSource(value)) return resolve(value);
    if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
    const entries = Object.entries(value);
    if (!entries.some(([, entry]) => isSecretProviderSecretSource(entry))) return null;
    return Effect.forEach(entries, ([serverHost, entry]) =>
      isSecretProviderSecretSource(entry)
        ? resolve(entry).pipe(Effect.map((secret) => [serverHost, secret] as const))
        : Effect.succeed([serverHost, entry] as const),
    ).pipe(Effect.map((resolved) => Object.fromEntries(resolved)));
  };
  const pending = Object.entries(hosts).flatMap(([kind, fields]) =>
    (SOURCE_CONTROL_HOST_SECRET_FIELDS.get(kind) ?? []).flatMap((field) => {
      const resolved = resolveField(fields[field]);
      return resolved === null ? [] : [{ kind, field, resolved }];
    }),
  );
  if (pending.length === 0) return Effect.succeed(hosts);
  return Effect.forEach(pending, ({ kind, field, resolved }) =>
    resolved.pipe(Effect.map((value) => ({ kind, field, value }))),
  ).pipe(
    Effect.map((fields) => {
      const next: Record<string, Readonly<Record<string, unknown>>> = { ...hosts };
      for (const { kind, field, value } of fields) next[kind] = { ...next[kind], [field]: value };
      return next;
    }),
  );
}
