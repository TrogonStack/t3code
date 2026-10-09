"use client";

import { Spinner } from "~/components/ui/spinner";

import {
  AlertTriangleIcon,
  ArrowUpCircleIcon,
  CopyIcon,
  DownloadIcon,
  ExternalLinkIcon,
  KeyRoundIcon,
  PlusIcon,
  Trash2Icon,
  TypeIcon,
  XIcon,
} from "lucide-react";
import { Lock as LockGlyph, LockOpen } from "lucide";
import * as Arr from "effect/Array";
import * as Result from "effect/Result";
import * as Schema from "effect/Schema";
import { useEffect, useRef, useState, type ReactElement, type ReactNode } from "react";
import {
  isProviderDriverKind,
  OnePasswordAccount,
  OnePasswordSecretReference,
  resolveProviderInstanceEnabled,
  type OnePasswordAccountSummary,
  type OnePasswordSecretSource,
  type ProviderInstanceConfig,
  type ProviderInstanceEnvironmentVariable,
  type ProviderInstanceId,
  type AcpRegistryUrlAuthAction,
  type EnvironmentId,
  type ProjectId,
  type ProviderDriverKind,
  type ServerProvider,
  type ServerProviderModel,
} from "@t3tools/contracts";

import {
  type CustomModelDefinition,
  readCustomModelEntries,
  toCustomModelSetting,
} from "@t3tools/shared/model";
import { cn } from "../../lib/utils";
import { useEnvironmentQuery } from "../../state/query";
import { serverEnvironment } from "../../state/server";
import { useCopyToClipboard } from "../../hooks/useCopyToClipboard";
import { normalizeProviderAccentColor } from "../../providerInstances";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { MorphIcon } from "~/components/MorphIcon";
import { DraftInput } from "../ui/draft-input";
import { Popover, PopoverPopup, PopoverTrigger } from "../ui/popover";
import { Select, SelectItem, SelectPopup, SelectTrigger, SelectValue } from "../ui/select";
import { Switch } from "../ui/switch";
import { stackedThreadToast, toastManager } from "../ui/toast";
import { Tooltip, TooltipPopup, TooltipTrigger } from "../ui/tooltip";
import type {
  ProviderClientDefinition,
  ProviderEnvironmentField,
} from "@t3tools/provider-core/client";
import { deriveProviderSettingsFields, ProviderSettingsForm } from "./ProviderSettingsForm";
import { ProviderModelsSection } from "./ProviderModelsSection";
import { ProviderInstanceIcon } from "../chat/ProviderInstanceIcon";
import { ProviderAccentColorPicker } from "./ProviderAccentColorPicker";
import { RedactedSensitiveText } from "./RedactedSensitiveText";
import { SettingsRow, SettingsSection } from "./settingsLayout";
import { AcpSessionManagementSection } from "./AcpSessionManagementSection";
import { FoldedSettingsSection } from "./FoldedSettingsSection";
import { readCodexSetupMode } from "./CodexSetupSection.logic";
import {
  getProviderVersionAdvisoryPresentation,
  PROVIDER_STATUS_STYLES,
  getProviderSummary,
  getProviderVersionLabel,
  type ProviderStatusKey,
} from "./providerStatus";

const ENVIRONMENT_VARIABLE_NAME_PATTERN = /^[a-zA-Z_][a-zA-Z0-9_]*$/;

function ProviderStatusDiagnostic({
  detail,
  children,
}: {
  detail: string | null;
  children: ReactElement;
}) {
  if (!detail) return children;
  return (
    <Tooltip>
      <TooltipTrigger render={children} />
      <TooltipPopup side="top">{detail}</TooltipPopup>
    </Tooltip>
  );
}

let environmentVariableDraftId = 0;
const nextEnvironmentVariableDraftId = () => `provider-env-${environmentVariableDraftId++}`;

type EnvironmentDraftSource = "plain" | "1password";

const PLAIN_SOURCE_OPTION = "plain";
const EMPTY_ONE_PASSWORD_ACCOUNTS: ReadonlyArray<OnePasswordAccountSummary> = [];
const ONE_PASSWORD_SOURCE_OPTION_PREFIX = "1password:";

/**
 * The source select's option for a row: a plain value, or the 1Password account
 * it reads from. When the account is typed instead, every 1Password row shares
 * one option.
 */
function environmentDraftSourceOption(row: EnvironmentDraftRow, typesAccount: boolean): string {
  if (row.source !== "1password") return PLAIN_SOURCE_OPTION;
  return `${ONE_PASSWORD_SOURCE_OPTION_PREFIX}${typesAccount ? "" : row.account}`;
}

function EnvironmentDraftSourceLabel(props: { readonly option: string }) {
  if (!props.option.startsWith(ONE_PASSWORD_SOURCE_OPTION_PREFIX)) {
    return (
      <span className="flex min-w-0 items-center gap-1.5">
        <TypeIcon className="size-3.5 shrink-0" />
        Value
      </span>
    );
  }
  const account = props.option.slice(ONE_PASSWORD_SOURCE_OPTION_PREFIX.length);
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <KeyRoundIcon className="size-3.5 shrink-0" />
      <span className="truncate">{account.length > 0 ? account : "1Password"}</span>
    </span>
  );
}

type EnvironmentDraftRow = {
  readonly id: string;
  readonly name: string;
  readonly source: EnvironmentDraftSource;
  readonly value: string;
  readonly reference: string;
  readonly account: string;
  readonly sensitive: boolean;
  readonly valueRedacted?: boolean;
  /** The name this row is saved under, so an unfinished edit can fall back to it. */
  readonly savedName?: string;
};

/**
 * Plain `op://` values were once read from 1Password. They are literals now,
 * so they open as a 1Password source that still needs its account.
 */
function isLegacyOnePasswordReference(variable: ProviderInstanceEnvironmentVariable): boolean {
  return (
    typeof variable.value === "string" &&
    variable.valueRedacted !== true &&
    variable.value.trim().startsWith("op://")
  );
}

function makeEnvironmentDraftRow(
  variable: ProviderInstanceEnvironmentVariable,
  index: number,
): EnvironmentDraftRow {
  const id = `${index}:${variable.name}`;
  if (typeof variable.value !== "string") {
    return {
      id,
      name: variable.name,
      source: "1password",
      savedName: variable.name,
      value: "",
      reference: variable.value.reference,
      account: variable.value.account,
      sensitive: false,
    };
  }
  if (isLegacyOnePasswordReference(variable)) {
    return {
      id,
      name: variable.name,
      source: "1password",
      savedName: variable.name,
      value: "",
      reference: variable.value.trim(),
      account: "",
      sensitive: false,
    };
  }
  return {
    id,
    name: variable.name,
    source: "plain",
    savedName: variable.name,
    value: variable.value,
    reference: "",
    account: "",
    sensitive: variable.sensitive,
    ...(variable.valueRedacted !== undefined ? { valueRedacted: variable.valueRedacted } : {}),
  };
}

const decodeOnePasswordSecretReference = Schema.decodeUnknownResult(OnePasswordSecretReference);
const decodeOnePasswordAccount = Schema.decodeUnknownResult(OnePasswordAccount);

/**
 * The 1Password source a draft row describes, or the message explaining why
 * it is not one yet.
 */
function onePasswordSourceFromDraft(
  row: EnvironmentDraftRow,
): Result.Result<OnePasswordSecretSource, string> {
  const reference = decodeOnePasswordSecretReference(row.reference);
  if (Result.isFailure(reference)) return Result.fail(reference.failure.message);
  const account = decodeOnePasswordAccount(row.account);
  if (Result.isFailure(account)) return Result.fail(account.failure.message);
  return Result.succeed({
    kind: "1password",
    reference: reference.success,
    account: account.success,
  });
}

function environmentValuesEqual(
  left: ProviderInstanceEnvironmentVariable["value"],
  right: ProviderInstanceEnvironmentVariable["value"],
): boolean {
  if (typeof left === "string" || typeof right === "string") return left === right;
  return (
    left.kind === right.kind && left.reference === right.reference && left.account === right.account
  );
}

function providerEnvironmentsEqual(
  left: ReadonlyArray<ProviderInstanceEnvironmentVariable>,
  right: ReadonlyArray<ProviderInstanceEnvironmentVariable>,
): boolean {
  return (
    left.length === right.length &&
    left.every((variable, index) => {
      const other = right[index];
      return (
        other !== undefined &&
        variable.name === other.name &&
        environmentValuesEqual(variable.value, other.value) &&
        variable.sensitive === other.sensitive &&
        variable.valueRedacted === other.valueRedacted
      );
    })
  );
}

/**
 * Read `customModels` from the opaque config blob. The concrete driver
 * schemas type it as `CustomModelSetting[]`, but it arrives here as
 * `Schema.Unknown`, so the shared reader does the shape checking.
 */
function readConfigCustomModels(config: unknown): ReadonlyArray<CustomModelDefinition> {
  if (config === null || typeof config !== "object") return [];
  return readCustomModelEntries((config as Record<string, unknown>).customModels);
}

function readConfigString(config: unknown, key: string): string | null {
  if (config === null || typeof config !== "object") return null;
  const value = (config as Record<string, unknown>)[key];
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : null;
}

/**
 * Set `key` to an arbitrary value on the opaque config blob. Unlike
 * provider settings field updates, does not drop empty-looking values — the
 * caller is responsible for deciding whether an empty array / empty
 * object should be stored explicitly (e.g. `customModels: []` is a
 * meaningful "user cleared their custom list" state distinct from
 * "driver default").
 */
function nextConfigBlobWithValue(
  config: unknown,
  key: string,
  value: unknown,
): Record<string, unknown> {
  const base: Record<string, unknown> =
    config !== null && typeof config === "object" ? { ...(config as Record<string, unknown>) } : {};
  base[key] = value;
  return base;
}

/**
 * Custom rows come from current settings so name/descriptor edits show
 * instantly; a bare entry falls back to the live row's driver-default
 * capabilities (the server fills those in on its next probe).
 */
export function deriveProviderModelsForDisplay(input: {
  readonly liveModels: ReadonlyArray<ServerProviderModel> | undefined;
  readonly customModels: ReadonlyArray<CustomModelDefinition>;
}): ReadonlyArray<ServerProviderModel> {
  const liveCustomModelsBySlug = new Map(
    Arr.filterMap(input.liveModels ?? [], (model) =>
      model.isCustom ? Result.succeed([model.slug, model] as const) : Result.failVoid,
    ),
  );
  const serverModels = input.liveModels?.filter((model) => !model.isCustom) ?? [];
  const customModels = input.customModels.map((entry) => ({
    slug: entry.slug,
    name: entry.name,
    isCustom: true,
    capabilities:
      entry.capabilities ?? liveCustomModelsBySlug.get(entry.slug)?.capabilities ?? null,
  }));
  return [...serverModels, ...customModels];
}

function ProviderAuthEmail(props: { readonly email: string | undefined }) {
  const email = props.email?.trim();
  if (!email) return null;

  return (
    <RedactedSensitiveText
      value={email}
      ariaLabel="Toggle account email visibility"
      revealTooltip="Click to reveal email"
      hideTooltip="Click to hide email"
      className="max-w-full truncate"
    />
  );
}

export function readProviderEnvironmentVariable(
  environment: ReadonlyArray<ProviderInstanceEnvironmentVariable> | undefined,
  name: string,
): ProviderInstanceEnvironmentVariable | undefined {
  return environment?.find((variable) => variable.name === name);
}

export function providerEnvironmentWithoutNames(
  environment: ReadonlyArray<ProviderInstanceEnvironmentVariable> | undefined,
  names: ReadonlySet<string>,
): ReadonlyArray<ProviderInstanceEnvironmentVariable> {
  return (environment ?? []).filter((variable) => !names.has(variable.name));
}

/**
 * Dedicated fields only edit plain values, so a variable read from a secret
 * store, or one that still has to become one, stays in the generic editor,
 * which can show its source.
 */
export function splitDedicatedProviderEnvironment(
  environment: ReadonlyArray<ProviderInstanceEnvironmentVariable> | undefined,
  names: ReadonlySet<string>,
): {
  readonly dedicated: ReadonlyArray<ProviderInstanceEnvironmentVariable>;
  readonly generic: ReadonlyArray<ProviderInstanceEnvironmentVariable>;
} {
  const dedicated: ProviderInstanceEnvironmentVariable[] = [];
  const generic: ProviderInstanceEnvironmentVariable[] = [];
  for (const variable of environment ?? []) {
    if (
      names.has(variable.name) &&
      typeof variable.value === "string" &&
      !isLegacyOnePasswordReference(variable)
    ) {
      dedicated.push(variable);
    } else generic.push(variable);
  }
  return { dedicated, generic };
}

export function nextProviderEnvironmentWithFieldValue(
  environment: ReadonlyArray<ProviderInstanceEnvironmentVariable> | undefined,
  field: ProviderEnvironmentField,
  value: string,
): ReadonlyArray<ProviderInstanceEnvironmentVariable> {
  const trimmed = value.trim();
  const next: ProviderInstanceEnvironmentVariable[] = [];
  let found = false;

  for (const variable of environment ?? []) {
    if (variable.name !== field.name) {
      next.push(variable);
      continue;
    }
    found = true;
    if (trimmed.length > 0) {
      next.push({
        name: variable.name,
        value: trimmed,
        sensitive: field.sensitive ?? true,
      });
    }
  }

  if (!found && trimmed.length > 0) {
    next.push({
      name: field.name,
      value: trimmed,
      sensitive: field.sensitive ?? true,
    });
  }

  return next;
}

function ProviderEnvironmentFieldRow(props: {
  readonly field: ProviderEnvironmentField;
  readonly variable: ProviderInstanceEnvironmentVariable | undefined;
  readonly idPrefix: string;
  readonly onCommit: (field: ProviderEnvironmentField, value: string) => void;
  readonly onRemove: (field: ProviderEnvironmentField) => void;
}) {
  const inputId = `${props.idPrefix}-environment-${props.field.name}`;
  const configuredValue = props.variable?.value;
  const readFromSecretStore =
    props.variable !== undefined &&
    (typeof configuredValue !== "string" || isLegacyOnePasswordReference(props.variable));
  const value =
    props.variable?.valueRedacted || typeof configuredValue !== "string" ? "" : configuredValue;
  const placeholder = readFromSecretStore
    ? "Read from 1Password - edit it under Environment"
    : props.variable?.valueRedacted
      ? "Stored secret - enter a new value to replace"
      : props.field.placeholder;

  return (
    <SettingsRow
      title={<label htmlFor={inputId}>{props.field.label}</label>}
      description={props.field.description}
      control={
        <div className="flex w-full min-w-0 items-center gap-2 @min-[32rem]/settings-row:w-56">
          <DraftInput
            id={inputId}
            size="sm"
            className="min-w-0 flex-1"
            type={props.field.sensitive === false ? undefined : "password"}
            autoComplete="off"
            value={value}
            onCommit={(next) => props.onCommit(props.field, next)}
            placeholder={placeholder}
            disabled={readFromSecretStore}
            spellCheck={false}
          />
          {props.variable && !readFromSecretStore ? (
            <Button
              type="button"
              size="icon-sm"
              variant="ghost-destructive"
              onClick={() => props.onRemove(props.field)}
              aria-label={`Clear ${props.field.label}`}
            >
              <XIcon className="size-3.5" />
            </Button>
          ) : null}
        </div>
      }
    />
  );
}

export function ProviderEnvironmentSection(props: {
  readonly environment: ReadonlyArray<ProviderInstanceEnvironmentVariable>;
  readonly onePasswordAccounts: ReadonlyArray<OnePasswordAccountSummary>;
  readonly onChange: (environment: ReadonlyArray<ProviderInstanceEnvironmentVariable>) => void;
}) {
  const [rows, setRows] = useState<ReadonlyArray<EnvironmentDraftRow>>(() =>
    props.environment.map(makeEnvironmentDraftRow),
  );
  const previousEnvironmentRef = useRef(props.environment);
  const lastPublishedEnvironmentRef = useRef<
    ReadonlyArray<ProviderInstanceEnvironmentVariable> | undefined
  >(undefined);

  useEffect(() => {
    const previousEnvironment = previousEnvironmentRef.current;
    const lastPublishedEnvironment = lastPublishedEnvironmentRef.current;
    previousEnvironmentRef.current = props.environment;
    lastPublishedEnvironmentRef.current = undefined;
    if (
      previousEnvironment === props.environment ||
      providerEnvironmentsEqual(previousEnvironment, props.environment) ||
      (lastPublishedEnvironment !== undefined &&
        providerEnvironmentsEqual(lastPublishedEnvironment, props.environment))
    ) {
      return;
    }
    setRows(props.environment.map(makeEnvironmentDraftRow));
  }, [props.environment]);

  const commitRows = (nextRows: ReadonlyArray<EnvironmentDraftRow>) => {
    const published: ProviderInstanceEnvironmentVariable[] = [];
    const savedNames = new Map<string, string>();
    for (const row of nextRows) {
      const name = row.name.trim();
      if (!ENVIRONMENT_VARIABLE_NAME_PATTERN.test(name)) {
        if (
          name.length > 0 ||
          row.value.length > 0 ||
          row.reference.length > 0 ||
          row.valueRedacted !== undefined
        ) {
          setRows(nextRows);
          return;
        }
        continue;
      }
      if (row.source === "1password") {
        const source = onePasswordSourceFromDraft(row);
        if (Result.isSuccess(source)) {
          published.push({ name, value: source.success, sensitive: false });
          savedNames.set(row.id, name);
          continue;
        }
        // Keep what is saved until the row is complete, so one unfinished row
        // never holds back edits to the others.
        const saved =
          row.savedName === undefined
            ? undefined
            : readProviderEnvironmentVariable(props.environment, row.savedName);
        if (saved !== undefined) published.push(saved);
        continue;
      }
      published.push({
        name,
        value: row.value,
        sensitive: row.sensitive,
        ...(row.valueRedacted !== undefined ? { valueRedacted: row.valueRedacted } : {}),
      });
      savedNames.set(row.id, name);
    }
    setRows(
      nextRows.map((row) => {
        const savedName = savedNames.get(row.id);
        return savedName === undefined ? row : { ...row, savedName };
      }),
    );
    lastPublishedEnvironmentRef.current = published;
    props.onChange(published);
  };

  const updateVariable = (id: string, patch: Partial<Omit<EnvironmentDraftRow, "id">>) => {
    const nextRows = rows.map((row) =>
      row.id === id
        ? {
            ...row,
            ...patch,
            ...(patch.value !== undefined ? { valueRedacted: false } : {}),
          }
        : row,
    );
    commitRows(nextRows);
  };

  const removeVariable = (id: string) => {
    const nextRows = rows.filter((row) => row.id !== id);
    commitRows(nextRows);
  };

  // Without any account from the server there is nothing to pick, so the
  // account is typed instead.
  const typesAccount = props.onePasswordAccounts.length === 0;
  // Accounts already saved stay pickable even when this server's `op` no
  // longer reports them, so opening settings never rewrites a row.
  const onePasswordAccounts = typesAccount
    ? [""]
    : Arr.dedupe([
        ...props.onePasswordAccounts.map(({ account }) => account as string),
        ...Arr.filterMap(rows, (row) =>
          row.source === "1password" && row.account.length > 0
            ? Result.succeed(row.account)
            : Result.failVoid,
        ),
      ]);
  const sourceOptions = [
    PLAIN_SOURCE_OPTION,
    ...onePasswordAccounts.map((account) => `${ONE_PASSWORD_SOURCE_OPTION_PREFIX}${account}`),
  ];
  const emailByAccount = new Map(
    props.onePasswordAccounts.map(({ account, email }) => [account as string, email]),
  );

  const addVariable = () =>
    setRows([
      ...rows,
      {
        id: nextEnvironmentVariableDraftId(),
        name: "",
        source: "plain",
        value: "",
        reference: "",
        account: "",
        sensitive: true,
      },
    ]);

  return (
    <SettingsRow
      title="Variables"
      description="API keys, base URLs, and other per-instance CLI settings."
      control={
        <Button type="button" size="sm" variant="outline" onClick={addVariable}>
          <PlusIcon className="size-3" />
          Add variable
        </Button>
      }
    >
      {rows.length > 0 ? (
        <div className="mt-3 flex min-w-0 flex-col gap-2 pb-2">
          {rows.map((variable, index) => {
            const isOnePassword = variable.source === "1password";
            const sourceIssue =
              isOnePassword &&
              (variable.name.length > 0 ||
                variable.reference.length > 0 ||
                variable.account.length > 0)
                ? Result.match(onePasswordSourceFromDraft(variable), {
                    onFailure: (message) => `Not saved yet. ${message}`,
                    onSuccess: () => undefined,
                  })
                : undefined;
            return (
              <div
                key={variable.id}
                className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-1.5 rounded-lg border p-2"
              >
                <DraftInput
                  size="sm"
                  font="mono"
                  className="min-w-0 flex-1"
                  value={variable.name}
                  onCommit={(name) => updateVariable(variable.id, { name: name.trim() })}
                  placeholder="VARIABLE_NAME"
                  spellCheck={false}
                  aria-label={`Environment variable name ${index + 1}`}
                />
                <Select
                  value={environmentDraftSourceOption(variable, typesAccount)}
                  onValueChange={(option) => {
                    if (
                      option === null ||
                      option === environmentDraftSourceOption(variable, typesAccount)
                    ) {
                      return;
                    }
                    if (!option.startsWith(ONE_PASSWORD_SOURCE_OPTION_PREFIX)) {
                      updateVariable(variable.id, { source: "plain", value: "", sensitive: true });
                      return;
                    }
                    updateVariable(variable.id, {
                      source: "1password",
                      account: typesAccount
                        ? variable.account
                        : option.slice(ONE_PASSWORD_SOURCE_OPTION_PREFIX.length),
                      value: "",
                      sensitive: false,
                    });
                  }}
                >
                  <SelectTrigger
                    size="sm"
                    className="w-48"
                    aria-label={`Environment variable source ${index + 1}`}
                  >
                    <SelectValue>
                      <EnvironmentDraftSourceLabel
                        option={environmentDraftSourceOption(variable, typesAccount)}
                      />
                    </SelectValue>
                  </SelectTrigger>
                  <SelectPopup>
                    {sourceOptions.map((option) => {
                      const email = emailByAccount.get(
                        option.slice(ONE_PASSWORD_SOURCE_OPTION_PREFIX.length),
                      );
                      return (
                        <SelectItem key={option} value={option}>
                          <span className="flex min-w-0 flex-col">
                            <EnvironmentDraftSourceLabel option={option} />
                            {email !== undefined ? (
                              <span className="truncate text-xs text-muted-foreground">
                                {email}
                              </span>
                            ) : null}
                          </span>
                        </SelectItem>
                      );
                    })}
                  </SelectPopup>
                </Select>
                <Button
                  type="button"
                  size="icon-micro"
                  variant="ghost-destructive"
                  onClick={() => removeVariable(variable.id)}
                  aria-label={`Remove environment variable ${variable.name || index + 1}`}
                >
                  <XIcon className="size-3" />
                </Button>
                {isOnePassword ? (
                  <>
                    <DraftInput
                      size="sm"
                      font="mono"
                      className={cn("min-w-0", !typesAccount && "col-span-2")}
                      value={variable.reference}
                      onCommit={(reference) =>
                        updateVariable(variable.id, { reference: reference.trim() })
                      }
                      placeholder="op://vault/item/field"
                      spellCheck={false}
                      aria-invalid={sourceIssue !== undefined || undefined}
                      aria-label={`Environment variable 1Password reference ${index + 1}`}
                    />
                    {typesAccount ? (
                      <DraftInput
                        size="sm"
                        font="mono"
                        className="w-48"
                        value={variable.account}
                        onCommit={(account) => updateVariable(variable.id, { account })}
                        placeholder="my.1password.com"
                        spellCheck={false}
                        aria-invalid={sourceIssue !== undefined || undefined}
                        aria-label={`Environment variable 1Password account ${index + 1}`}
                      />
                    ) : null}
                  </>
                ) : (
                  <>
                    <DraftInput
                      size="sm"
                      font="mono"
                      className="col-span-2 min-w-0"
                      value={variable.valueRedacted ? "" : variable.value}
                      onCommit={(value) => updateVariable(variable.id, { value })}
                      type={variable.sensitive ? "password" : undefined}
                      autoComplete="off"
                      placeholder={
                        variable.valueRedacted
                          ? "Stored secret, enter a new value to replace"
                          : "value"
                      }
                      spellCheck={false}
                      aria-label={`Environment variable value ${index + 1}`}
                    />
                    <Tooltip>
                      <TooltipTrigger
                        render={
                          <Button
                            type="button"
                            size="icon-micro"
                            variant="ghost-muted"
                            onClick={() => {
                              const sensitive = !variable.sensitive;
                              updateVariable(variable.id, {
                                sensitive,
                                ...(sensitive && variable.valueRedacted === undefined
                                  ? {}
                                  : {
                                      valueRedacted: sensitive ? variable.valueRedacted : false,
                                    }),
                              });
                            }}
                            aria-pressed={variable.sensitive}
                            aria-label={`Mark environment variable ${variable.name || index + 1} as sensitive`}
                          >
                            <MorphIcon
                              className="size-3"
                              icon={variable.sensitive ? LockGlyph : LockOpen}
                            />
                          </Button>
                        }
                      />
                      <TooltipPopup side="top">
                        {variable.sensitive ? "Sensitive, stored separately" : "Plain text"}
                      </TooltipPopup>
                    </Tooltip>
                  </>
                )}
                {sourceIssue !== undefined ? (
                  <p className="col-span-3 text-xs text-destructive">{sourceIssue}</p>
                ) : null}
              </div>
            );
          })}
          <p className="text-xs text-muted-foreground">
            Sensitive values are stored separately and never returned to the app. 1Password
            references are read with the 1Password CLI each time the provider starts.
          </p>
        </div>
      ) : null}
    </SettingsRow>
  );
}

interface ProviderInstanceCardProps {
  readonly instanceId: ProviderInstanceId;
  readonly instance: ProviderInstanceConfig;
  readonly driverOption: ProviderClientDefinition | undefined;
  readonly liveProvider: ServerProvider | undefined;
  readonly mode: "list" | "editor";
  readonly selected?: boolean | undefined;
  readonly onSelect?: (() => void) | undefined;
  readonly readOnly?: boolean | undefined;
  readonly canWriteSettings?: boolean;
  readonly onUpdate: (nextInstance: ProviderInstanceConfig) => void;
  /**
   * Pass `undefined` to hide the delete footer entirely. Built-in default
   * instance slots use `undefined` — they can't be deleted without losing
   * the slot, and their "reset to defaults" affordance lives on an outer
   * reset button instead. Explicit `| undefined` in the type accommodates
   * `exactOptionalPropertyTypes: true`, where an absent key and
   * `{ onDelete: undefined }` are treated as distinct shapes.
   */
  readonly onDelete?: (() => void) | undefined;
  /**
   * Optional outer reset button rendered next to the driver icon. Built-in
   * default slots supply a reset-to-factory control here; custom instances
   * omit it.
   */
  readonly headerAction?: ReactNode | undefined;
  readonly setup?: ReactNode;
  readonly runtime?: ReactNode;
  readonly hiddenModels: ReadonlyArray<string>;
  readonly favoriteModels: ReadonlyArray<string>;
  readonly modelOrder: ReadonlyArray<string>;
  readonly onHiddenModelsChange: (next: ReadonlyArray<string>) => void;
  readonly onFavoriteModelsChange: (next: ReadonlyArray<string>) => void;
  readonly onModelOrderChange: (next: ReadonlyArray<string>) => void;
  readonly onRunUpdate?: (() => void) | undefined;
  readonly onInstallRecommended?: (() => void) | undefined;
  readonly isUpdating?: boolean | undefined;
  readonly onAcceptUrlAuth?: ((action: AcpRegistryUrlAuthAction) => void) | undefined;
  readonly environmentId?: EnvironmentId | undefined;
  readonly acpProjects?:
    | ReadonlyArray<{
        readonly id: ProjectId;
        readonly title: string;
        readonly workspaceRoot: string;
      }>
    | undefined;
}

const EMPTY_ACP_PROJECTS: NonNullable<ProviderInstanceCardProps["acpProjects"]> = [];

/**
 * Renders one provider instance as either a compact selectable list row or
 * the full editor shown beside that list. Both modes use the same enabled
 * state and provider metadata.
 *
 * Behavior notes:
 *   - `liveProvider` is matched by the caller via `instanceId`; when no
 *     match is available (e.g. the server hasn't checked it yet, or the
 *     driver is not shipped by the current build) the card still renders
 *     with a neutral "checking" summary.
 *   - Unknown drivers (`driverOption === undefined`) get a read-only
 *     notice instead of editable fields, so fork instances round-trip
 *     without accidentally destroying their config.
 *   - The enabled Switch writes to the envelope's `instance.enabled`
 *     field, which is the single enabled flag: the server folds any legacy
 *     driver-specific `config.enabled` into the envelope on load and both
 *     sides resolve through `resolveProviderInstanceEnabled` (an explicit
 *     false wins, then envelope, then config, then the driver default).
 */
export function ProviderInstanceCard({
  instanceId,
  instance,
  driverOption,
  liveProvider,
  mode,
  selected = false,
  onSelect,
  readOnly = false,
  canWriteSettings = true,
  onUpdate,
  onDelete,
  headerAction,
  setup,
  runtime,
  hiddenModels,
  favoriteModels,
  modelOrder,
  onHiddenModelsChange,
  onFavoriteModelsChange,
  onModelOrderChange,
  onRunUpdate,
  onInstallRecommended,
  isUpdating = false,
  onAcceptUrlAuth,
  environmentId,
  acpProjects = EMPTY_ACP_PROJECTS,
}: ProviderInstanceCardProps) {
  const enabled = resolveProviderInstanceEnabled(instance);
  const compatibility = enabled ? liveProvider?.compatibilityAdvisory : undefined;
  // A locally disabled provider reads "Disabled" with a muted dot even if its
  // last server status is stale. Enabled providers use the server status.
  const statusKey: ProviderStatusKey = enabled
    ? ((liveProvider?.status as ProviderStatusKey | undefined) ?? "warning")
    : "disabled";
  const statusStyle = PROVIDER_STATUS_STYLES[statusKey];
  const summary = enabled
    ? getProviderSummary(liveProvider)
    : { headline: "Disabled", detail: null };
  const authEmail = liveProvider?.auth.email?.trim();
  const isAuthenticated = enabled && liveProvider?.auth.status === "authenticated";
  const authLabel =
    enabled && liveProvider?.auth.status === "authenticated"
      ? (liveProvider.auth.label ?? liveProvider.auth.type ?? null)
      : null;
  const versionLabel = getProviderVersionLabel(liveProvider?.version);
  const versionAdvisory = getProviderVersionAdvisoryPresentation(
    liveProvider?.versionAdvisory,
    liveProvider?.compatibilityAdvisory,
    enabled,
  );
  const updateCommand = versionAdvisory?.updateCommand ?? null;
  const updateState = liveProvider?.updateState;
  // The server reports each update step. `isUpdating` also covers the moment
  // between the click and the server's first report.
  const updateProgress = isUpdating
    ? ((updateState?.status === "queued" || updateState?.status === "running"
        ? updateState.message
        : null) ?? "Starting update")
    : null;
  const updateProblem =
    !isUpdating && (updateState?.status === "failed" || updateState?.status === "unchanged")
      ? updateState.message
      : null;
  const hasCompatibilityWarning =
    compatibility !== undefined &&
    compatibility.status !== "supported" &&
    compatibility.status !== "unknown";
  const VersionAdvisoryIcon = hasCompatibilityWarning ? AlertTriangleIcon : ArrowUpCircleIcon;
  const onRunVersionAction = readOnly
    ? undefined
    : versionAdvisory?.targetVersion
      ? onInstallRecommended
      : onRunUpdate;
  const urlAuthAction = liveProvider?.auth.action;
  const displayName =
    instance.displayName?.trim() || driverOption?.label || String(instance.driver);
  const accentColor = normalizeProviderAccentColor(instance.accentColor);
  const { copyToClipboard } = useCopyToClipboard<{ providerName: string }>({
    onCopy: ({ providerName }) => {
      toastManager.add({
        type: "success",
        title: `${providerName} update command copied`,
        description: "Run it in a terminal when you are ready to update.",
      });
    },
    onError: (error, { providerName }) => {
      toastManager.add(
        stackedThreadToast({
          type: "error",
          title: `Could not copy ${providerName} update command`,
          description: error.message,
        }),
      );
    },
  });

  // Narrow `instance.driver` for callers that key on the closed
  // `ProviderDriverKind` union (e.g. `normalizeModelSlug`'s alias table). Custom
  // fork drivers pass through as `null` and those callers fall back to
  // verbatim behaviour.
  const driverKind: ProviderDriverKind | null = isProviderDriverKind(instance.driver)
    ? instance.driver
    : null;
  const customModels =
    instance.driver === "antigravity" ? [] : readConfigCustomModels(instance.config);
  // Server-returned models may lag behind settings writes. Treat server
  // models as the source for built-ins only; custom rows come directly
  // from the current instance config so add/remove reflects immediately.
  const modelsForDisplay = deriveProviderModelsForDisplay({
    liveModels: liveProvider?.models,
    customModels,
  });
  const updateDisplayName = (value: string) => {
    const trimmed = value.trim();
    const { displayName: _omit, ...rest } = instance;
    onUpdate(
      trimmed.length > 0
        ? ({ ...rest, displayName: trimmed } as ProviderInstanceConfig)
        : (rest as ProviderInstanceConfig),
    );
  };

  const updateEnabled = (value: boolean) => {
    onUpdate({ ...instance, enabled: value });
  };

  const updateAccentColor = (value: string) => {
    const normalized = normalizeProviderAccentColor(value);
    const { accentColor: _omit, ...rest } = instance;
    onUpdate(
      normalized
        ? ({ ...rest, accentColor: normalized } as ProviderInstanceConfig)
        : (rest as ProviderInstanceConfig),
    );
  };

  const updateConfig = (nextConfig: Record<string, unknown> | undefined) => {
    const { config: _omit, ...rest } = instance;
    onUpdate(
      nextConfig !== undefined
        ? ({ ...rest, config: nextConfig } as ProviderInstanceConfig)
        : (rest as ProviderInstanceConfig),
    );
  };

  const updateCustomModels = (next: ReadonlyArray<CustomModelDefinition>) => {
    const nextConfig = nextConfigBlobWithValue(
      instance.config,
      "customModels",
      next.map(toCustomModelSetting),
    );
    const { config: _omit, ...rest } = instance;
    onUpdate({ ...rest, config: nextConfig } as ProviderInstanceConfig);
  };

  const updateEnvironment = (environment: ReadonlyArray<ProviderInstanceEnvironmentVariable>) => {
    const cleaned = environment.filter((variable) => variable.name.trim().length > 0);
    const { environment: _omit, ...rest } = instance;
    onUpdate(
      cleaned.length > 0
        ? ({ ...rest, environment: cleaned } as ProviderInstanceConfig)
        : (rest as ProviderInstanceConfig),
    );
  };
  // Drivers that need a named secret (Cursor's API key) get a dedicated field;
  // the generic editor only shows the remaining variables.
  const environmentFields = driverOption?.environmentFields ?? [];
  const environmentFieldNames = new Set(environmentFields.map((field) => field.name));
  const onePasswordAccounts = useEnvironmentQuery(
    environmentId === undefined
      ? null
      : serverEnvironment.onePasswordAccounts({ environmentId, input: {} }),
  );
  const { dedicated: dedicatedEnvironment, generic: genericEnvironment } =
    splitDedicatedProviderEnvironment(instance.environment, environmentFieldNames);
  const updateGenericEnvironment = (
    environment: ReadonlyArray<ProviderInstanceEnvironmentVariable>,
  ) => {
    const genericNames = new Set(environment.map((variable) => variable.name));
    updateEnvironment([
      ...providerEnvironmentWithoutNames(dedicatedEnvironment, genericNames),
      ...environment,
    ]);
  };
  const updateEnvironmentField = (field: ProviderEnvironmentField, value: string) => {
    updateEnvironment(nextProviderEnvironmentWithFieldValue(instance.environment, field, value));
  };
  const removeEnvironmentField = (field: ProviderEnvironmentField) => {
    updateEnvironment(providerEnvironmentWithoutNames(instance.environment, new Set([field.name])));
  };

  const titleIconNode = (
    <ProviderInstanceIcon
      driverKind={driverKind ?? instance.driver}
      displayName={displayName}
      accentColor={accentColor}
      acpRegistryAgentId={
        readConfigString(instance.config, "source") === "local"
          ? undefined
          : (readConfigString(instance.config, "agentId") ?? undefined)
      }
      acpRegistryIconUrl={
        readConfigString(instance.config, "source") === "local"
          ? undefined
          : (readConfigString(instance.config, "registryIconUrl") ?? undefined)
      }
      showBadge={Boolean(accentColor)}
      className="size-5"
      iconClassName="size-4 text-foreground/80"
      badgeClassName="right-[-0.125rem] bottom-[-0.125rem] h-3 min-w-3 px-0.5 text-5xs"
    />
  );

  const titleTailNode = headerAction ? (
    <span className="inline-flex h-5 w-5 shrink-0 items-center justify-center">{headerAction}</span>
  ) : null;

  const versionCodeNode = versionLabel ? (
    <code className="text-xs text-muted-foreground">{versionLabel}</code>
  ) : null;

  // Healthy and disabled rows read fine from their text; only trouble gets a dot.
  const statusDotNode =
    statusKey === "warning" || statusKey === "error" ? (
      <span className={cn("size-1.5 shrink-0 rounded-full", statusStyle.dot)} aria-hidden />
    ) : null;
  const needsAttention = statusKey === "warning" || statusKey === "error";
  const statusDiagnostic = hasCompatibilityWarning && needsAttention ? summary.detail : null;
  // Keep compatibility copy compact; the version popover carries the explanation.
  const inlineStatusDetail = hasCompatibilityWarning
    ? compatibility?.status === "broken"
      ? "Incompatible"
      : compatibility?.status === "unsupported"
        ? "Unsupported"
        : "Limited support"
    : summary.detail;
  const editorStatusNode =
    isAuthenticated && authEmail ? (
      <>
        {needsAttention ? statusDotNode : null}
        <span>Authenticated as</span>
        <ProviderAuthEmail email={authEmail} />
        {authLabel ? <span>· {authLabel}</span> : null}
        {inlineStatusDetail ? (
          <span className="min-w-0 [overflow-wrap:anywhere]">· {inlineStatusDetail}</span>
        ) : null}
      </>
    ) : (
      <>
        {statusDotNode}
        <span>{summary.headline}</span>
        {inlineStatusDetail ? (
          <span className="min-w-0 [overflow-wrap:anywhere]">· {inlineStatusDetail}</span>
        ) : null}
      </>
    );
  const versionAdvisoryNode = versionAdvisory ? (
    <Popover>
      <Tooltip>
        <TooltipTrigger
          render={
            <PopoverTrigger
              render={
                <Button
                  type="button"
                  size={mode === "list" ? "icon-micro" : "icon-xs"}
                  variant="ghost-muted"
                  className={mode === "list" ? "pointer-events-auto relative shrink-0" : undefined}
                  aria-label={`${updateProgress ? "Updating" : versionAdvisory.title} — view details`}
                >
                  {updateProgress ? (
                    <Spinner tone="muted" {...(mode === "list" ? { size: "sm" as const } : {})} />
                  ) : (
                    <VersionAdvisoryIcon
                      className={cn(
                        mode === "list" && "size-3.5",
                        hasCompatibilityWarning && "text-warning",
                      )}
                    />
                  )}
                </Button>
              }
            />
          }
        />
        <TooltipPopup side="top">
          {updateProgress ? "Updating" : versionAdvisory.title}
        </TooltipPopup>
      </Tooltip>
      <PopoverPopup side="bottom" align="end" width="md" aria-label={versionAdvisory.title}>
        <div className="grid min-w-0 gap-3">
          <div className="grid gap-0.5">
            <p className="text-sm font-semibold leading-tight text-foreground">
              {versionAdvisory.title}
            </p>
            <p
              className={cn(
                "text-xs leading-snug",
                versionAdvisory.emphasis === "strong" ? "text-warning" : "text-muted-foreground",
              )}
            >
              {versionAdvisory.detail}
            </p>
          </div>
          {onRunVersionAction ? (
            <Button
              type="button"
              size="xs"
              variant="outline"
              className="w-full"
              disabled={isUpdating}
              onClick={onRunVersionAction}
            >
              {isUpdating ? <Spinner /> : <DownloadIcon />}
              {isUpdating
                ? "Updating"
                : versionAdvisory.targetVersion
                  ? `Install ${getProviderVersionLabel(versionAdvisory.targetVersion)}`
                  : "Update now"}
            </Button>
          ) : null}
          {updateProgress || updateProblem ? (
            <p
              aria-live="polite"
              className={cn(
                "text-xs leading-snug [overflow-wrap:anywhere]",
                updateProblem ? "text-warning" : "text-muted-foreground",
              )}
            >
              {updateProgress ?? updateProblem}
            </p>
          ) : null}
          {onRunVersionAction && updateCommand ? (
            <div className="flex items-center gap-2 text-3xs font-medium uppercase tracking-wider text-muted-foreground">
              <span aria-hidden className="h-px flex-1 bg-border" />
              or, update manually using
              <span aria-hidden className="h-px flex-1 bg-border" />
            </div>
          ) : null}
          {updateCommand ? (
            <div className="flex min-w-0 items-center gap-1 rounded-md border border-border/70 bg-muted/40 py-0.5 pr-0.5 pl-2">
              <code className="min-w-0 flex-1 truncate font-mono text-2xs text-foreground">
                {updateCommand}
              </code>
              <Tooltip>
                <TooltipTrigger
                  render={
                    <Button
                      type="button"
                      size="icon-xs"
                      variant="ghost-muted"
                      className="shrink-0"
                      onClick={() => copyToClipboard(updateCommand, { providerName: displayName })}
                      aria-label="Copy update command"
                    >
                      <CopyIcon className="size-3" />
                    </Button>
                  }
                />
                <TooltipPopup side="top">Copy command</TooltipPopup>
              </Tooltip>
            </div>
          ) : null}
        </div>
      </PopoverPopup>
    </Popover>
  ) : null;

  if (mode === "list") {
    return (
      <div
        data-slot="settings-row"
        className={cn(
          "group flex min-h-18 items-center gap-3 px-3 py-3 transition-colors sm:px-4",
          selected ? "bg-muted/45" : "hover:bg-muted/25",
        )}
      >
        <div
          className={cn(
            "pointer-events-none relative flex min-w-0 flex-1 items-start gap-3 rounded-md text-left transition-opacity",
            !enabled && !selected && "opacity-60 group-hover:opacity-100",
          )}
        >
          <button
            type="button"
            className="pointer-events-auto absolute inset-0 cursor-pointer rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring"
            onClick={onSelect}
            aria-label={`Select ${displayName}`}
            aria-pressed={selected}
          />
          {titleIconNode}
          <span className="min-w-0 flex-1">
            <span className="flex min-w-0 items-center gap-2">
              <span className="truncate text-sm font-medium text-foreground">{displayName}</span>
              {versionLabel ? (
                <code className="max-w-24 shrink-0 truncate text-xs text-muted-foreground">
                  {versionLabel}
                </code>
              ) : null}
              {versionAdvisoryNode}
            </span>
            <span className="mt-0.5 flex items-start gap-1.5 text-xs leading-normal text-muted-foreground/80">
              {/* The dot describes provider health, not the update in progress. */}
              {statusDotNode && !updateProgress ? (
                <span className="flex h-[1.45em] shrink-0 items-center">{statusDotNode}</span>
              ) : null}
              <ProviderStatusDiagnostic detail={statusDiagnostic}>
                <span
                  tabIndex={statusDiagnostic ? 0 : undefined}
                  aria-live="polite"
                  className="pointer-events-auto line-clamp-2 [overflow-wrap:anywhere]"
                >
                  {updateProgress ? (
                    `Updating · ${updateProgress}`
                  ) : (
                    <>
                      {summary.headline}
                      {needsAttention && inlineStatusDetail ? ` · ${inlineStatusDetail}` : null}
                    </>
                  )}
                </span>
              </ProviderStatusDiagnostic>
            </span>
          </span>
        </div>
        <span className="flex h-5 shrink-0 items-center">
          <Switch
            checked={enabled}
            disabled={readOnly}
            onCheckedChange={(checked) => updateEnabled(Boolean(checked))}
            aria-label={`Enable ${displayName}`}
          />
        </span>
      </div>
    );
  }

  const editorHeaderAction = (
    <div className="flex shrink-0 items-center gap-1.5">
      {driverOption?.badgeLabel ? (
        <Badge variant="warning" size="sm" className="shrink-0">
          {driverOption.badgeLabel}
        </Badge>
      ) : null}
      {versionCodeNode}
      {versionAdvisoryNode}
      <span
        inert={readOnly}
        aria-disabled={readOnly || undefined}
        className={cn("inline-flex items-center gap-1", readOnly && "opacity-50")}
      >
        {titleTailNode}
        {onDelete ? (
          <Button
            type="button"
            size="icon-xs"
            variant="ghost-destructive"
            disabled={readOnly}
            onClick={onDelete}
            aria-label={`Delete instance ${instanceId}`}
          >
            <Trash2Icon />
          </Button>
        ) : null}
      </span>
    </div>
  );

  const runtimeFields = driverOption ? (
    <ProviderSettingsForm
      definition={driverOption}
      value={instance.config}
      idPrefix={`provider-instance-${instanceId}`}
      variant="settings"
      onChange={updateConfig}
    />
  ) : (
    <SettingsRow
      title="Driver"
      description={
        <span>
          This instance uses <code className="text-foreground">{String(instance.driver)}</code>,
          which is not available in this build. Its configuration is preserved.
        </span>
      }
    />
  );

  return (
    <>
      <SettingsSection title={displayName} icon={titleIconNode} headerAction={editorHeaderAction}>
        <SettingsRow
          title="Display name"
          status={
            <>
              <ProviderStatusDiagnostic detail={statusDiagnostic}>
                <div
                  tabIndex={statusDiagnostic ? 0 : undefined}
                  className="flex min-w-0 flex-wrap items-baseline gap-x-1.5"
                >
                  {editorStatusNode}
                </div>
              </ProviderStatusDiagnostic>
              {urlAuthAction && onAcceptUrlAuth ? (
                <div className="grid max-w-xl gap-1.5 pt-1 text-xs">
                  <p>{urlAuthAction.message}</p>
                  <code className="break-all text-2xs">{urlAuthAction.url}</code>
                  <Button
                    render={
                      <a
                        href={urlAuthAction.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        onClick={() => onAcceptUrlAuth(urlAuthAction)}
                      />
                    }
                    size="xs"
                    variant="outline"
                    className="w-fit"
                    disabled={readOnly}
                  >
                    <ExternalLinkIcon />
                    Continue authentication
                  </Button>
                </div>
              ) : null}
            </>
          }
          control={
            <div
              inert={readOnly}
              aria-disabled={readOnly || undefined}
              className={cn(
                "flex w-full min-w-0 items-center justify-end gap-2 @min-[32rem]/settings-row:w-auto",
                readOnly && "opacity-50 select-none",
              )}
            >
              <ProviderAccentColorPicker
                layout="inline"
                displayName={displayName}
                value={accentColor}
                onCommit={updateAccentColor}
                commitDelayMs={120}
              />
              <DraftInput
                id={`provider-instance-${instanceId}-display-name`}
                size="sm"
                className="min-w-0 flex-1 @min-[32rem]/settings-row:w-56"
                value={instance.displayName ?? ""}
                onCommit={updateDisplayName}
                placeholder={driverOption?.label ?? "Instance label"}
                spellCheck={false}
              />
            </div>
          }
        />
      </SettingsSection>

      {setup || environmentFields.length > 0 ? (
        <SettingsSection title="Setup">
          {setup}
          <div
            inert={readOnly}
            aria-disabled={readOnly || undefined}
            className={readOnly ? "opacity-50 select-none" : undefined}
          >
            {environmentFields.length > 0 ? (
              <>
                {environmentFields.map((field) => (
                  <ProviderEnvironmentFieldRow
                    key={field.name}
                    field={field}
                    variable={readProviderEnvironmentVariable(instance.environment, field.name)}
                    idPrefix={`provider-instance-${instanceId}`}
                    onCommit={updateEnvironmentField}
                    onRemove={removeEnvironmentField}
                  />
                ))}
              </>
            ) : null}
          </div>
        </SettingsSection>
      ) : null}

      {instance.driver === "codex" && readCodexSetupMode(instance.config) === "managed" ? (
        <div
          inert={readOnly}
          aria-disabled={readOnly || undefined}
          className={readOnly ? "opacity-50 select-none" : undefined}
        >
          <FoldedSettingsSection
            key={instanceId}
            id={`provider-instance-${instanceId}-runtime`}
            title="Runtime"
            headerPlacement="outside"
          >
            {runtime ?? runtimeFields}
          </FoldedSettingsSection>
        </div>
      ) : !driverOption || deriveProviderSettingsFields(driverOption).length > 0 ? (
        <SettingsSection
          title="Runtime"
          inert={readOnly}
          aria-disabled={readOnly || undefined}
          className={readOnly ? "opacity-50 select-none" : undefined}
        >
          {runtimeFields}
        </SettingsSection>
      ) : null}

      <SettingsSection
        title="Environment"
        inert={readOnly}
        aria-disabled={readOnly || undefined}
        className={readOnly ? "opacity-50 select-none" : undefined}
      >
        <ProviderEnvironmentSection
          environment={genericEnvironment}
          onePasswordAccounts={onePasswordAccounts.data ?? EMPTY_ONE_PASSWORD_ACCOUNTS}
          onChange={updateGenericEnvironment}
        />
      </SettingsSection>
      {environmentId !== undefined && liveProvider?.driver === "acpRegistry" ? (
        <AcpSessionManagementSection
          environmentId={environmentId}
          instanceId={instanceId}
          provider={liveProvider}
          projects={acpProjects}
          readOnly={readOnly}
        />
      ) : null}

      {driverOption !== undefined ? (
        <SettingsSection title="Models">
          <div className="px-3 py-3 sm:px-4">
            <p className="mb-3 text-xs text-muted-foreground">
              Favorites, visibility, and ordering are saved on this device. Custom models are saved
              on the selected environment.
            </p>
            <ProviderModelsSection
              canManageCustomModels={!readOnly}
              canWritePreferences={canWriteSettings}
              instanceId={instanceId}
              driverKind={driverKind}
              models={modelsForDisplay}
              customModels={customModels}
              hiddenModels={hiddenModels}
              favoriteModels={favoriteModels}
              modelOrder={modelOrder}
              onChange={updateCustomModels}
              onHiddenModelsChange={onHiddenModelsChange}
              onFavoriteModelsChange={onFavoriteModelsChange}
              onModelOrderChange={onModelOrderChange}
            />
          </div>
        </SettingsSection>
      ) : null}
    </>
  );
}
