import { useAtomValue } from "@effect/atom-react";
import {
  OnePasswordAccount,
  OnePasswordSecretReference,
  SecretProviderId,
  SecretProviderReference,
  isSecretProviderSecretSource,
  type EnvironmentId,
  type ProviderSecretSource,
  type SecretProviderConfig,
  type SecretProviderKind,
  type SecretProviderSecretSource,
  type SettingsSecretValue,
  type UnifiedSettings,
} from "@t3tools/contracts";
import * as Result from "effect/Result";
import * as Schema from "effect/Schema";
import { useNavigate } from "@tanstack/react-router";
import {
  AlertTriangleIcon,
  CheckIcon,
  KeyRoundIcon,
  PlusIcon,
  RefreshCwIcon,
  TypeIcon,
} from "lucide-react";
import { useState } from "react";

import { cn } from "../../lib/utils";
import { serverEnvironment } from "../../state/server";
import { useAtomCommand } from "../../state/use-atom-command";
import { Button } from "../ui/button";
import { DraftInput } from "../ui/draft-input";
import { Input } from "../ui/input";
import { Select, SelectItem, SelectPopup, SelectTrigger, SelectValue } from "../ui/select";
import { Spinner } from "../ui/spinner";
import { Tooltip, TooltipPopup, TooltipTrigger } from "../ui/tooltip";
import {
  SECRET_PROVIDER_REFERENCE_PLACEHOLDERS,
  secretProviderReferenceIssue,
  secretProviderSummary,
} from "./SecretProviderDialog.logic";
import { SecretInput } from "./SecretInput";
import { SecretProviderKindIcon } from "./SecretProviderKindIcon";

type EnvironmentDraftSource = "plain" | "1password" | "secret-provider";

export type SecretProviders = UnifiedSettings["secretProviders"];

const PLAIN_SOURCE_OPTION = "plain";
export const EMPTY_SECRET_PROVIDERS: SecretProviders = {};
const SECRET_PROVIDER_SOURCE_OPTION_PREFIX = "secret-provider:";
const ADD_SECRET_PROVIDER_OPTION = "add-secret-provider";
/** Rows saved before named providers keep their own account; new rows cannot pick it. */
const ONE_PASSWORD_SOURCE_OPTION_PREFIX = "1password:";

function environmentDraftSourceOption(
  row: Pick<EnvironmentDraftRow, "source" | "provider" | "account">,
): string {
  switch (row.source) {
    case "plain":
      return PLAIN_SOURCE_OPTION;
    case "secret-provider":
      return `${SECRET_PROVIDER_SOURCE_OPTION_PREFIX}${row.provider}`;
    case "1password":
      return `${ONE_PASSWORD_SOURCE_OPTION_PREFIX}${row.account}`;
  }
}

type EnvironmentSourceOption = {
  readonly value: string;
  readonly label: string;
  readonly detail?: string;
  readonly secret: boolean;
  readonly kind?: SecretProviderKind;
};

function lookupSecretProvider(
  secretProviders: SecretProviders,
  id: string,
): SecretProviderConfig | undefined {
  return Object.hasOwn(secretProviders, id)
    ? secretProviders[SecretProviderId.make(id)]
    : undefined;
}

/**
 * Value, then every configured provider. A row also keeps the option it is
 * already on: a provider that was removed, or its legacy 1Password account.
 */
function environmentSourceOptions(
  row: Pick<EnvironmentDraftRow, "source" | "provider" | "account">,
  secretProviders: SecretProviders,
): ReadonlyArray<EnvironmentSourceOption> {
  const options: EnvironmentSourceOption[] = [
    { value: PLAIN_SOURCE_OPTION, label: "Value", secret: false },
    ...Object.entries(secretProviders).map(([id, config]) => ({
      value: `${SECRET_PROVIDER_SOURCE_OPTION_PREFIX}${id}`,
      label: config.label,
      detail: secretProviderSummary(config),
      secret: true,
      kind: config.kind,
    })),
  ];
  if (
    row.source === "secret-provider" &&
    lookupSecretProvider(secretProviders, row.provider) === undefined
  ) {
    options.push({
      value: environmentDraftSourceOption(row),
      label: row.provider,
      detail: "Not configured in Settings → Secrets",
      secret: true,
    });
  }
  if (row.source === "1password") {
    options.push({
      value: environmentDraftSourceOption(row),
      label: row.account.length > 0 ? row.account : "1Password",
      detail: "1Password account set on this variable",
      secret: true,
      kind: "1password",
    });
  }
  return options;
}

function EnvironmentSourceOptionLabel(props: { readonly option: EnvironmentSourceOption }) {
  const Icon = props.option.secret ? KeyRoundIcon : TypeIcon;
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      {props.option.kind !== undefined ? (
        <SecretProviderKindIcon kind={props.option.kind} className="size-3.5 shrink-0" />
      ) : (
        <Icon className="size-3.5 shrink-0" />
      )}
      <span className="truncate">{props.option.label}</span>
    </span>
  );
}

export function EnvironmentSourceSelect(props: {
  readonly row: Pick<EnvironmentDraftRow, "source" | "provider" | "account">;
  readonly secretProviders: SecretProviders;
  readonly className: string;
  readonly ariaLabel: string;
  readonly onChange: (patch: Partial<Omit<EnvironmentDraftRow, "id">>) => void;
}) {
  const navigate = useNavigate();
  const sourceOptions = environmentSourceOptions(props.row, props.secretProviders);
  const selectedOption = environmentDraftSourceOption(props.row);
  const selectedSourceOption =
    sourceOptions.find((option) => option.value === selectedOption) ?? sourceOptions[0]!;
  return (
    <Select
      value={selectedOption}
      onValueChange={(option) => {
        if (option === null || option === selectedOption) return;
        if (option === ADD_SECRET_PROVIDER_OPTION) {
          void navigate({ to: "/settings/secrets" });
          return;
        }
        if (option === PLAIN_SOURCE_OPTION) {
          props.onChange({ source: "plain", value: "", sensitive: true });
          return;
        }
        if (option.startsWith(SECRET_PROVIDER_SOURCE_OPTION_PREFIX)) {
          props.onChange({
            source: "secret-provider",
            provider: option.slice(SECRET_PROVIDER_SOURCE_OPTION_PREFIX.length),
            value: "",
            sensitive: false,
          });
        }
      }}
    >
      <SelectTrigger size="sm" className={props.className} aria-label={props.ariaLabel}>
        <SelectValue>
          <EnvironmentSourceOptionLabel option={selectedSourceOption} />
        </SelectValue>
      </SelectTrigger>
      <SelectPopup>
        {sourceOptions.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            <span className="flex min-w-0 flex-col">
              <EnvironmentSourceOptionLabel option={option} />
              {option.detail !== undefined ? (
                <span className="truncate text-xs text-muted-foreground">{option.detail}</span>
              ) : null}
            </span>
          </SelectItem>
        ))}
        <SelectItem value={ADD_SECRET_PROVIDER_OPTION}>
          <span className="flex min-w-0 items-center gap-1.5 text-muted-foreground">
            <PlusIcon className="size-3.5 shrink-0" />
            <span className="truncate">Add secret provider</span>
          </span>
        </SelectItem>
      </SelectPopup>
    </Select>
  );
}

export function referencePlaceholderFor(
  row: Pick<EnvironmentDraftRow, "source" | "provider">,
  secretProviders: SecretProviders,
) {
  return row.source === "1password"
    ? SECRET_PROVIDER_REFERENCE_PLACEHOLDERS["1password"]
    : SECRET_PROVIDER_REFERENCE_PLACEHOLDERS[
        lookupSecretProvider(secretProviders, row.provider)?.kind ?? "1password"
      ];
}

export type EnvironmentDraftRow = {
  readonly id: string;
  readonly name: string;
  readonly source: EnvironmentDraftSource;
  readonly value: string;
  readonly reference: string;
  readonly account: string;
  readonly provider: string;
  readonly sensitive: boolean;
  readonly valueRedacted?: boolean;
  /** The name this row is saved under, so an unfinished edit can fall back to it. */
  readonly savedName?: string;
};

const decodeOnePasswordSecretReference = Schema.decodeUnknownResult(OnePasswordSecretReference);
const decodeOnePasswordAccount = Schema.decodeUnknownResult(OnePasswordAccount);
const decodeSecretProviderReference = Schema.decodeUnknownResult(SecretProviderReference);

/**
 * The secret source a draft row describes, or the message explaining why it
 * is not one yet. A row may name a provider that is not configured: it saves,
 * and resolves once a provider with that name is added again.
 */
export function secretSourceFromDraft(
  row: Pick<EnvironmentDraftRow, "source" | "reference" | "account" | "provider">,
  secretProviders: SecretProviders,
): Result.Result<ProviderSecretSource, string> {
  if (row.source === "1password") {
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
  return secretProviderSourceFromDraft(row, secretProviders);
}

function secretProviderSourceFromDraft(
  row: Pick<EnvironmentDraftRow, "reference" | "provider">,
  secretProviders: SecretProviders,
): Result.Result<SecretProviderSecretSource, string> {
  if (row.provider.length === 0) return Result.fail("Choose a secret provider.");
  const reference = decodeSecretProviderReference(row.reference);
  if (Result.isFailure(reference)) return Result.fail(reference.failure.message);
  const config = lookupSecretProvider(secretProviders, row.provider);
  const issue =
    config === undefined ? undefined : secretProviderReferenceIssue(config.kind, reference.success);
  if (issue !== undefined) return Result.fail(issue);
  return Result.succeed({
    kind: "secret-provider",
    provider: SecretProviderId.make(row.provider),
    reference: reference.success,
  });
}

type SecretCheckState =
  | { readonly status: "checking" }
  | { readonly status: "resolved" }
  | { readonly status: "failed"; readonly detail: string };

interface RowSecretCheck {
  readonly sourceKey: string;
  readonly state: SecretCheckState;
}

export function useSecretSourceChecks(environmentId: EnvironmentId | undefined) {
  const canCheck = useAtomValue(
    serverEnvironment.checkSecretSource.permissionAtom(environmentId ?? null),
  );
  const checkSecretSource = useAtomCommand(serverEnvironment.checkSecretSource);
  const [checks, setChecks] = useState<Readonly<Record<string, RowSecretCheck>>>({});

  const run = async (rowId: string, source: ProviderSecretSource) => {
    if (environmentId === undefined) return;
    const sourceKey = JSON.stringify(source);
    setChecks((previous) => ({
      ...previous,
      [rowId]: { sourceKey, state: { status: "checking" } },
    }));
    const response = await checkSecretSource({ environmentId, input: { source } });
    const state: SecretCheckState =
      response._tag !== "Success"
        ? { status: "failed", detail: "Could not reach the server" }
        : response.value._tag === "Resolved"
          ? { status: "resolved" }
          : { status: "failed", detail: response.value.detail };
    setChecks((previous) =>
      previous[rowId]?.sourceKey === sourceKey
        ? { ...previous, [rowId]: { sourceKey, state } }
        : previous,
    );
  };

  return { canCheck, checks, run };
}

export function SecretCheckButton(props: {
  readonly label: string;
  readonly state: SecretCheckState | undefined;
  readonly disabledReason: string | undefined;
  readonly onCheck: () => void;
}) {
  const { state, disabledReason } = props;
  const checking = state?.status === "checking";
  const description =
    disabledReason ??
    (state === undefined
      ? "Check that the secret resolves"
      : state.status === "checking"
        ? "Checking the secret"
        : state.status === "resolved"
          ? "Secret resolved"
          : state.detail);
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span className="inline-flex">
            <Button
              type="button"
              size="icon-micro"
              variant="ghost-muted"
              disabled={disabledReason !== undefined || checking}
              onClick={props.onCheck}
              aria-label={
                state?.status === "failed" && disabledReason === undefined
                  ? `${props.label} failed: ${state.detail}`
                  : state?.status === "resolved" && disabledReason === undefined
                    ? `${props.label} resolved`
                    : props.label
              }
            >
              {checking ? (
                <Spinner />
              ) : state?.status === "resolved" ? (
                <CheckIcon className="size-3 text-success-foreground" />
              ) : state?.status === "failed" ? (
                <AlertTriangleIcon className="size-3 text-destructive" />
              ) : (
                <RefreshCwIcon className="size-3" />
              )}
            </Button>
          </span>
        }
      />
      <TooltipPopup side="top">{description}</TooltipPopup>
    </Tooltip>
  );
}

/** A secret field being edited: the literal typed so far, or a provider and its reference. */
export type SecretFieldDraft =
  | { readonly kind: "literal"; readonly text: string }
  | { readonly kind: "reference"; readonly provider: string; readonly reference: string };

export const EMPTY_SECRET_FIELD_DRAFT: SecretFieldDraft = { kind: "literal", text: "" };

/** A saved literal arrives redacted, so only a saved reference is shown back for editing. */
export function secretFieldDraft(value: unknown): SecretFieldDraft {
  return isSecretProviderSecretSource(value)
    ? { kind: "reference", provider: value.provider, reference: value.reference }
    : EMPTY_SECRET_FIELD_DRAFT;
}

/**
 * What saving the draft writes, or why it cannot be saved yet. An empty literal is
 * `undefined`: nothing was typed, and the caller decides whether that keeps or clears.
 */
export function secretFieldSubmission(
  draft: SecretFieldDraft,
  secretProviders: SecretProviders,
): Result.Result<SettingsSecretValue | undefined, string> {
  if (draft.kind === "literal") {
    const text = draft.text.trim();
    return Result.succeed(text.length > 0 ? text : undefined);
  }
  return secretProviderSourceFromDraft(
    { provider: draft.provider, reference: draft.reference.trim() },
    secretProviders,
  );
}

/** A saved credential setting holds a literal or names a secret. */
export function hasSavedCredential(value: unknown): boolean {
  return isSecretProviderSecretSource(value) || (typeof value === "string" && value.length > 0);
}

type SecretFieldProps = {
  readonly id: string;
  readonly label: string;
  readonly environmentId: EnvironmentId | undefined;
  readonly secretProviders: SecretProviders;
  /** What is saved; a saved literal is usually redacted. */
  readonly value: SettingsSecretValue | undefined;
  /** A literal is saved and redacted. Defaults to any non-empty saved literal. */
  readonly configured?: boolean | undefined;
  /** Mask the literal. A plain literal is shown back as saved. */
  readonly sensitive?: boolean | undefined;
  readonly placeholder?: string | undefined;
  readonly disabled?: boolean | undefined;
  readonly describedBy?: string | undefined;
  readonly className?: string | undefined;
} & (
  | {
      /** Saves a finished literal or reference as soon as it is entered. */
      readonly onCommit: (next: SettingsSecretValue) => void;
      readonly draft?: never;
      readonly onDraftChange?: never;
    }
  | {
      /** Holds the draft in the caller, for a form that saves it later. */
      readonly draft: SecretFieldDraft;
      readonly onDraftChange: (draft: SecretFieldDraft) => void;
      readonly onCommit?: never;
    }
);

/**
 * A credential input that holds a literal or reads it from a secret provider. The
 * source picker is always shown, so the field reads as secret-capable even before a
 * provider exists, and a reference can be checked against the server.
 */
export function SecretField(props: SecretFieldProps) {
  const { onCommit } = props;
  const savedKey = JSON.stringify(props.value ?? "");
  const [pending, setPending] = useState<{
    readonly savedKey: string;
    readonly draft: SecretFieldDraft;
  } | null>(null);
  const draft =
    props.draft ?? (pending?.savedKey === savedKey ? pending.draft : secretFieldDraft(props.value));
  const setDraft = (next: SecretFieldDraft) => {
    if (props.onDraftChange !== undefined) props.onDraftChange(next);
    else setPending({ savedKey, draft: next });
  };
  const secretChecks = useSecretSourceChecks(props.environmentId);
  const configured =
    props.configured ?? (typeof props.value === "string" && props.value.length > 0);
  const savedLiteral = !configured && typeof props.value === "string" ? props.value : "";
  const row =
    draft.kind === "literal"
      ? { source: "plain" as const, provider: "", account: "" }
      : { source: "secret-provider" as const, provider: draft.provider, account: "" };
  const submission = secretFieldSubmission(draft, props.secretProviders);
  const source =
    Result.isSuccess(submission) && isSecretProviderSecretSource(submission.success)
      ? submission.success
      : undefined;
  const issue =
    Result.isFailure(submission) && draft.kind === "reference" && draft.reference.trim() !== ""
      ? submission.failure
      : undefined;
  const sourceKey = source === undefined ? undefined : JSON.stringify(source);
  const check = secretChecks.checks[props.id];
  const checkState = check !== undefined && check.sourceKey === sourceKey ? check.state : undefined;

  const commitReference = () => {
    if (onCommit !== undefined && source !== undefined && sourceKey !== savedKey) onCommit(source);
  };
  const changeSource = (patch: Partial<Omit<EnvironmentDraftRow, "id">>) => {
    if (patch.source === "secret-provider" && patch.provider !== undefined) {
      setDraft({ kind: "reference", provider: patch.provider, reference: "" });
      return;
    }
    setDraft(EMPTY_SECRET_FIELD_DRAFT);
    if (onCommit !== undefined && isSecretProviderSecretSource(props.value)) onCommit("");
  };

  return (
    <div className={cn("grid min-w-0 gap-1", props.className)}>
      <div className="flex min-w-0 items-center gap-2">
        <EnvironmentSourceSelect
          row={row}
          secretProviders={props.secretProviders}
          className="w-28 shrink-0"
          ariaLabel={`${props.label} source`}
          onChange={changeSource}
        />
        {draft.kind === "reference" ? (
          <>
            <Input
              id={props.id}
              size="sm"
              font="mono"
              className="min-w-0 flex-1"
              autoComplete="off"
              spellCheck={false}
              disabled={props.disabled}
              value={draft.reference}
              placeholder={referencePlaceholderFor(row, props.secretProviders)}
              aria-invalid={issue !== undefined || undefined}
              aria-label={`${props.label} secret reference`}
              aria-describedby={props.describedBy}
              onChange={(event) => setDraft({ ...draft, reference: event.target.value })}
              onBlur={commitReference}
              onKeyDown={(event) => {
                if (event.key !== "Enter" || onCommit === undefined) return;
                event.preventDefault();
                commitReference();
              }}
            />
            {secretChecks.canCheck && props.environmentId !== undefined ? (
              <SecretCheckButton
                label={`Check secret for ${props.label}`}
                state={checkState}
                disabledReason={Result.isFailure(submission) ? submission.failure : undefined}
                onCheck={() => {
                  if (source !== undefined) void secretChecks.run(props.id, source);
                }}
              />
            ) : null}
          </>
        ) : props.draft !== undefined ? (
          <SecretInput
            id={props.id}
            size="sm"
            className="min-w-0 flex-1"
            configured={configured}
            value={draft.text}
            disabled={props.disabled}
            placeholder={props.placeholder}
            aria-describedby={props.describedBy}
            onChange={(event) => setDraft({ kind: "literal", text: event.target.value })}
          />
        ) : props.sensitive === false ? (
          <DraftInput
            id={props.id}
            size="sm"
            className="min-w-0 flex-1"
            autoComplete="off"
            spellCheck={false}
            value={savedLiteral}
            disabled={props.disabled}
            placeholder={props.placeholder}
            aria-describedby={props.describedBy}
            onCommit={(next) => onCommit?.(next)}
          />
        ) : (
          <SecretInput
            id={props.id}
            size="sm"
            className="min-w-0 flex-1"
            configured={configured}
            value={savedLiteral}
            disabled={props.disabled}
            placeholder={props.placeholder}
            aria-describedby={props.describedBy}
            onCommit={(next) => onCommit?.(next)}
          />
        )}
      </div>
      {issue !== undefined ? <p className="text-xs text-destructive">{issue}</p> : null}
    </div>
  );
}
