import type { EnvironmentId, SecretProviderConfig, SecretProviderId } from "@t3tools/contracts";
import * as Result from "effect/Result";
import { useState } from "react";

import { useUpdateEnvironmentSettings } from "../../hooks/useSettings";
import { useEnvironmentQuery } from "../../state/query";
import { serverEnvironment } from "../../state/server";
import {
  Autocomplete,
  AutocompleteInput,
  AutocompleteItem,
  AutocompleteList,
  AutocompletePopup,
} from "../ui/autocomplete";
import { Button } from "../ui/button";
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogPopup,
  DialogTitle,
} from "../ui/dialog";
import { Input } from "../ui/input";
import { Label } from "../ui/label";
import { Select, SelectItem, SelectPopup, SelectTrigger, SelectValue } from "../ui/select";
import {
  DEFAULT_OPENBAO_MOUNT,
  EMPTY_SECRET_PROVIDER_DRAFT,
  SECRET_PROVIDER_KINDS,
  SECRET_PROVIDER_KIND_LABELS,
  draftFromSecretProvider,
  secretProviderFromDraft,
  type SecretProviderDraft,
  type SecretProviderDraftIssues,
  type SupportedSecretProviderKind,
} from "./SecretProviderDialog.logic";
import { SecretInput } from "./SecretInput";
import { SecretProviderKindIcon } from "./SecretProviderKindIcon";

const NAME_PLACEHOLDERS: Readonly<Record<SupportedSecretProviderKind, string>> = {
  one_password: "Personal",
  openbao: "Homelab",
  aws_secrets_manager: "Production",
};

const AUTH_OPTIONS = [
  { auth: "token", label: "Token", description: "The token below, stored with the provider." },
  {
    auth: "env",
    label: "BAO_TOKEN on the server",
    description: "The server process must have BAO_TOKEN set.",
  },
  {
    auth: "cli",
    label: "bao CLI login",
    description:
      "Runs bao print token on the server, so bao must be installed and logged in there.",
  },
] as const;

function KindLabel({ kind }: { readonly kind: SupportedSecretProviderKind }) {
  return (
    <span className="flex items-center gap-2">
      <SecretProviderKindIcon kind={kind} className="size-4 shrink-0" />
      {SECRET_PROVIDER_KIND_LABELS[kind]}
    </span>
  );
}

function FieldError({
  id,
  message,
}: {
  readonly id: string;
  readonly message: string | undefined;
}) {
  return message === undefined ? null : (
    <p id={id} role="alert" className="text-xs text-destructive">
      {message}
    </p>
  );
}

/** Mounted per open, so each open starts from a fresh draft. */
export function SecretProviderDialog({
  onClose,
  environmentId,
  environmentLabel,
  existingIds,
  existing,
}: {
  readonly onClose: () => void;
  readonly environmentId: EnvironmentId;
  readonly environmentLabel: string;
  readonly existingIds: ReadonlySet<string>;
  readonly existing?: { readonly id: SecretProviderId; readonly config: SecretProviderConfig };
}) {
  const updateSettings = useUpdateEnvironmentSettings(environmentId);
  const [draft, setDraft] = useState<SecretProviderDraft>(
    existing ? draftFromSecretProvider(existing.config) : EMPTY_SECRET_PROVIDER_DRAFT,
  );
  const [submitted, setSubmitted] = useState(false);
  const result = secretProviderFromDraft(
    draft,
    existingIds,
    existing ? { mode: "edit", id: existing.id } : { mode: "add" },
  );
  const issues: SecretProviderDraftIssues = Result.isFailure(result) ? result.failure : {};
  const fieldError = (field: keyof SecretProviderDraftIssues) =>
    submitted ? issues[field] : undefined;
  const nameError = fieldError("name");
  const accountError = fieldError("account");
  const addressError = fieldError("address");
  const mountError = fieldError("mount");
  const namespaceError = fieldError("namespace");
  const tokenError = fieldError("token");
  const regionError = fieldError("region");
  const profileError = fieldError("profile");
  const authOption = AUTH_OPTIONS.find((option) => option.auth === draft.auth);
  const accounts = useEnvironmentQuery(
    draft.kind === "one_password"
      ? serverEnvironment.onePasswordAccounts({ environmentId, input: {} })
      : null,
  );
  const listedAccounts = accounts.data ?? [];

  const update = (patch: Partial<SecretProviderDraft>) => setDraft({ ...draft, ...patch });
  const save = () => {
    setSubmitted(true);
    if (Result.isFailure(result)) return;
    updateSettings({ secretProviders: { [result.success.id]: result.success.config } });
    onClose();
  };

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogPopup className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {existing ? `Edit ${existing.config.label}` : "Add a secret provider"}
          </DialogTitle>
          <DialogDescription>
            Provider variables on {environmentLabel} can read values from it by name. Values are
            read on that server when a provider starts.
          </DialogDescription>
        </DialogHeader>
        <DialogPanel>
          <form
            id="secret-provider-form"
            className="grid gap-4"
            onSubmit={(event) => {
              event.preventDefault();
              save();
            }}
          >
            <div className="grid gap-1.5">
              <Label htmlFor="secret-provider-kind">Kind</Label>
              <Select
                value={draft.kind}
                disabled={existing !== undefined}
                onValueChange={(kind) => {
                  if (kind !== null) update({ kind });
                }}
              >
                <SelectTrigger id="secret-provider-kind">
                  <SelectValue>
                    <KindLabel kind={draft.kind} />
                  </SelectValue>
                </SelectTrigger>
                <SelectPopup>
                  {SECRET_PROVIDER_KINDS.map((kind) => (
                    <SelectItem key={kind} value={kind}>
                      <KindLabel kind={kind} />
                    </SelectItem>
                  ))}
                </SelectPopup>
              </Select>
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="secret-provider-name">Name</Label>
              <Input
                id="secret-provider-name"
                placeholder={NAME_PLACEHOLDERS[draft.kind]}
                value={draft.name}
                onChange={(event) => update({ name: event.target.value })}
                maxLength={64}
                aria-invalid={nameError !== undefined}
                aria-describedby={
                  nameError !== undefined ? "secret-provider-name-error" : undefined
                }
                autoFocus
              />
              {nameError !== undefined ? (
                <p
                  id="secret-provider-name-error"
                  role="alert"
                  className="text-xs text-destructive"
                >
                  {nameError}
                </p>
              ) : null}
            </div>
            {draft.kind === "one_password" ? (
              <div className="grid gap-1.5">
                <Label htmlFor="secret-provider-account">Account</Label>
                <Autocomplete
                  items={listedAccounts}
                  itemToStringValue={(item) => item.account}
                  mode="none"
                  openOnInputClick
                  value={draft.account}
                  onValueChange={(account) => update({ account })}
                >
                  <AutocompleteInput
                    id="secret-provider-account"
                    placeholder="my.1password.com"
                    spellCheck={false}
                    showTrigger={listedAccounts.length > 0}
                    aria-invalid={accountError !== undefined}
                    aria-describedby={
                      accountError !== undefined ? "secret-provider-account-error" : undefined
                    }
                  />
                  {listedAccounts.length > 0 ? (
                    <AutocompletePopup>
                      <AutocompleteList className="max-h-72">
                        {listedAccounts.map((item) => (
                          <AutocompleteItem key={item.account} value={item}>
                            <span className="min-w-0 truncate text-sm font-medium">
                              {item.account}
                            </span>
                            <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
                              {item.email}
                            </span>
                          </AutocompleteItem>
                        ))}
                      </AutocompleteList>
                    </AutocompletePopup>
                  ) : null}
                </Autocomplete>
                <FieldError id="secret-provider-account-error" message={accountError} />
                <p className="text-xs text-muted-foreground">
                  Accounts the op CLI is signed into are suggested. Type any other shorthand,
                  sign-in address without https://, or ID.
                </p>
              </div>
            ) : null}
            {draft.kind === "openbao" ? (
              <>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-address">Address</Label>
                  <Input
                    id="secret-provider-address"
                    placeholder="https://bao.example:8200"
                    value={draft.address}
                    onChange={(event) => update({ address: event.target.value })}
                    spellCheck={false}
                    aria-invalid={addressError !== undefined}
                    aria-describedby={
                      addressError !== undefined ? "secret-provider-address-error" : undefined
                    }
                  />
                  <FieldError id="secret-provider-address-error" message={addressError} />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-mount">Mount</Label>
                  <Input
                    id="secret-provider-mount"
                    placeholder={DEFAULT_OPENBAO_MOUNT}
                    value={draft.mount}
                    onChange={(event) => update({ mount: event.target.value })}
                    spellCheck={false}
                    aria-invalid={mountError !== undefined}
                    aria-describedby={
                      mountError !== undefined ? "secret-provider-mount-error" : undefined
                    }
                  />
                  <FieldError id="secret-provider-mount-error" message={mountError} />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-namespace">Namespace (optional)</Label>
                  <Input
                    id="secret-provider-namespace"
                    placeholder="admin/platform"
                    value={draft.namespace}
                    onChange={(event) => update({ namespace: event.target.value })}
                    spellCheck={false}
                    aria-invalid={namespaceError !== undefined}
                    aria-describedby={
                      namespaceError !== undefined ? "secret-provider-namespace-error" : undefined
                    }
                  />
                  <FieldError id="secret-provider-namespace-error" message={namespaceError} />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-auth">Authentication</Label>
                  <Select
                    value={draft.auth}
                    onValueChange={(auth) => {
                      if (auth !== null) update({ auth });
                    }}
                  >
                    <SelectTrigger id="secret-provider-auth">
                      <SelectValue>{authOption?.label}</SelectValue>
                    </SelectTrigger>
                    <SelectPopup>
                      {AUTH_OPTIONS.map((option) => (
                        <SelectItem key={option.auth} value={option.auth}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectPopup>
                  </Select>
                  <p className="text-xs text-muted-foreground">{authOption?.description}</p>
                </div>
                {draft.auth === "token" ? (
                  <div className="grid gap-1.5">
                    <Label htmlFor="secret-provider-token">Token</Label>
                    <SecretInput
                      id="secret-provider-token"
                      placeholder="s.xxxxxxxxxxxxxxxxxxxxxxxx"
                      value={draft.token}
                      configured={draft.storedToken.length > 0}
                      onChange={(event) => update({ token: event.target.value })}
                      aria-invalid={tokenError !== undefined}
                      aria-describedby={
                        tokenError !== undefined ? "secret-provider-token-error" : undefined
                      }
                    />
                    <FieldError id="secret-provider-token-error" message={tokenError} />
                  </div>
                ) : null}
              </>
            ) : null}
            {draft.kind === "aws_secrets_manager" ? (
              <>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-region">Region</Label>
                  <Input
                    id="secret-provider-region"
                    placeholder="us-east-1"
                    value={draft.region}
                    onChange={(event) => update({ region: event.target.value })}
                    spellCheck={false}
                    aria-invalid={regionError !== undefined}
                    aria-describedby={
                      regionError !== undefined ? "secret-provider-region-error" : undefined
                    }
                  />
                  <FieldError id="secret-provider-region-error" message={regionError} />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-profile">Profile (optional)</Label>
                  <Input
                    id="secret-provider-profile"
                    placeholder="production"
                    value={draft.profile}
                    onChange={(event) => update({ profile: event.target.value })}
                    spellCheck={false}
                    aria-invalid={profileError !== undefined}
                    aria-describedby={
                      profileError !== undefined ? "secret-provider-profile-error" : undefined
                    }
                  />
                  <FieldError id="secret-provider-profile-error" message={profileError} />
                  <p className="text-xs text-muted-foreground">
                    Credentials come from the aws CLI configuration on the server: this profile, or
                    its default chain when empty.
                  </p>
                </div>
              </>
            ) : null}
          </form>
        </DialogPanel>
        <DialogFooter variant="bare">
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            form="secret-provider-form"
            disabled={submitted && Result.isFailure(result)}
          >
            {existing ? "Save changes" : "Add provider"}
          </Button>
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  );
}
