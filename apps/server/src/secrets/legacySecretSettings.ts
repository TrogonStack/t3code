type Json = Record<string, unknown>;

const isRecord = (value: unknown): value is Json =>
  value !== null && typeof value === "object" && !Array.isArray(value);

const LEGACY_PROVIDER_KINDS: Readonly<Record<string, string>> = {
  "1password": "one_password",
  openbao: "openbao",
  "aws-secrets-manager": "aws_secrets_manager",
};

const LEGACY_PROVIDER_FIELDS: Readonly<Record<string, ReadonlyArray<string>>> = {
  "1password": ["account"],
  openbao: ["address", "mount", "namespace", "auth", "token"],
  "aws-secrets-manager": ["region", "profile"],
};

const PROVIDER_ID_MAX_CHARS = 64;
const PROVIDER_LABEL_MAX_CHARS = 64;

const pickFields = (entry: Json, fields: ReadonlyArray<string>): Json =>
  Object.fromEntries(
    fields.flatMap((field) => (entry[field] === undefined ? [] : [[field, entry[field]]])),
  );

const migrateProvider = (entry: unknown): unknown => {
  if (!isRecord(entry) || typeof entry.kind !== "string" || Object.hasOwn(entry, "config")) {
    return entry;
  }
  const kind = LEGACY_PROVIDER_KINDS[entry.kind];
  if (kind === undefined) return entry;
  return {
    kind,
    label: entry.label,
    config: pickFields(entry, LEGACY_PROVIDER_FIELDS[entry.kind]!),
  };
};

const legacyReference = (
  value: unknown,
):
  | { readonly provider?: string; readonly account?: string; readonly reference: string }
  | undefined => {
  if (!isRecord(value) || typeof value.reference !== "string") return undefined;
  if (value.kind === "secret-provider" && typeof value.provider === "string") {
    return { provider: value.provider, reference: value.reference };
  }
  if (value.kind === "1password" && typeof value.account === "string") {
    return { account: value.account, reference: value.reference };
  }
  return undefined;
};

const accountSlug = (account: string) =>
  `one-password-${account.replace(/[^A-Za-z0-9_-]/g, "-")}`.slice(0, PROVIDER_ID_MAX_CHARS);

/**
 * Rewrites the fork's secret shapes in raw settings into the shared secret provider shapes:
 * flat provider entries become `{ kind, label, config }` envelopes, and credential values that
 * named a provider or a 1Password account become `{ provider, reference }`. Anything already in
 * the new shape passes through untouched.
 */
export const migrateLegacySecretSettings = (
  settings: Json,
): { readonly settings: Json; readonly changed: boolean } => {
  let changed = false;

  const secretProviders: Json = {};
  const rawProviders = isRecord(settings.secretProviders) ? settings.secretProviders : undefined;
  if (rawProviders !== undefined) {
    for (const [id, entry] of Object.entries(rawProviders)) {
      const migrated = migrateProvider(entry);
      if (migrated !== entry) changed = true;
      secretProviders[id] = migrated;
    }
  }

  const accountProviders = new Map<string, string>();
  for (const [id, entry] of Object.entries(secretProviders)) {
    if (isRecord(entry) && entry.kind === "one_password" && isRecord(entry.config)) {
      const account = entry.config.account;
      if (typeof account === "string" && !accountProviders.has(account)) {
        accountProviders.set(account, id);
      }
    }
  }

  const providerForAccount = (account: string) => {
    const existing = accountProviders.get(account);
    if (existing !== undefined) return existing;
    let id = accountSlug(account);
    for (let suffix = 2; Object.hasOwn(secretProviders, id); suffix += 1) {
      id = `${accountSlug(account).slice(0, PROVIDER_ID_MAX_CHARS - 4)}-${suffix}`;
    }
    secretProviders[id] = {
      kind: "one_password",
      label: `1Password ${account}`.slice(0, PROVIDER_LABEL_MAX_CHARS),
      config: { account },
    };
    accountProviders.set(account, id);
    return id;
  };

  const toSecretReference = (value: unknown) => {
    const legacy = legacyReference(value);
    if (legacy === undefined) return undefined;
    changed = true;
    return {
      provider: legacy.provider ?? providerForAccount(legacy.account!),
      reference: legacy.reference,
    };
  };

  const walk = (value: unknown): unknown => {
    const reference = toSecretReference(value);
    if (reference !== undefined) return reference;
    if (Array.isArray(value)) return value.map(walk);
    if (!isRecord(value)) return value;
    return Object.fromEntries(Object.entries(value).map(([key, inner]) => [key, walk(inner)]));
  };

  const migrateInstance = (instance: unknown): unknown => {
    if (!isRecord(instance)) return instance;
    const { environment, ...rest } = instance;
    if (!Array.isArray(environment)) return walk(instance);
    return {
      ...(walk(rest) as Json),
      environment: environment.map((variable) => {
        if (!isRecord(variable)) return variable;
        const reference = toSecretReference(variable.value);
        if (reference === undefined) return walk(variable);
        const { value: _legacy, ...others } = variable;
        return { ...(walk(others) as Json), value: "", secret: reference };
      }),
    };
  };

  const rest: Json = {};
  for (const [key, value] of Object.entries(settings)) {
    if (key === "secretProviders") continue;
    if (key === "providerInstances" && isRecord(value)) {
      rest[key] = Object.fromEntries(
        Object.entries(value).map(([id, instance]) => [id, migrateInstance(instance)]),
      );
      continue;
    }
    rest[key] = walk(value);
  }

  if (!changed) return { settings, changed: false };
  return {
    settings: {
      ...rest,
      ...(rawProviders === undefined && Object.keys(secretProviders).length === 0
        ? {}
        : { secretProviders }),
    },
    changed: true,
  };
};
