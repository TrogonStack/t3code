import type { EnvironmentId, OpenBaoAuth, SecretProviderKind } from "@t3tools/contracts";
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
  secretProviderFromDraft,
  type SecretProviderDraft,
  type SecretProviderDraftField,
} from "./AddSecretProviderDialog.logic";
import { SecretInput } from "./SecretInput";
import { SecretProviderKindIcon } from "./SecretProviderKindIcon";

const KIND_OPTIONS: ReadonlyArray<{ readonly kind: SecretProviderKind; readonly label: string }> = [
  { kind: "1password", label: "1Password CLI" },
  { kind: "openbao", label: "OpenBao" },
  { kind: "aws-secrets-manager", label: "AWS Secrets Manager" },
];

const NAME_PLACEHOLDERS: Readonly<Record<SecretProviderKind, string>> = {
  "1password": "Personal",
  openbao: "Homelab",
  "aws-secrets-manager": "Production",
};

function SecretProviderKindLabel({ kind }: { readonly kind: SecretProviderKind }) {
  const label = KIND_OPTIONS.find((option) => option.kind === kind)?.label;
  return (
    <span className="flex items-center gap-2">
      <SecretProviderKindIcon kind={kind} className="size-4 shrink-0" aria-hidden />
      {label}
    </span>
  );
}

const AUTH_OPTIONS: ReadonlyArray<{
  readonly auth: OpenBaoAuth;
  readonly label: string;
  readonly description: string;
}> = [
  {
    auth: "token",
    label: "Token",
    description: "The token below, stored with the provider.",
  },
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
];

/**
 * Adds a named secret provider to one environment. Provider variables on that
 * environment pick it by name; values are read on the server when a provider starts.
 */
export function AddSecretProviderDialog({
  open,
  onOpenChange,
  environmentId,
  environmentLabel,
  existingIds,
}: {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly environmentId: EnvironmentId;
  readonly environmentLabel: string;
  readonly existingIds: ReadonlySet<string>;
}) {
  const updateSettings = useUpdateEnvironmentSettings(environmentId);
  const [draft, setDraft] = useState<SecretProviderDraft>(EMPTY_SECRET_PROVIDER_DRAFT);
  const [submitted, setSubmitted] = useState(false);
  const accounts = useEnvironmentQuery(
    draft.kind === "1password"
      ? serverEnvironment.onePasswordAccounts({ environmentId, input: {} })
      : null,
  );
  const listedAccounts = accounts.data ?? [];
  const result = secretProviderFromDraft(draft, existingIds);
  const issues = Result.isFailure(result) ? result.failure : {};
  const fieldError = (field: SecretProviderDraftField) => (submitted ? issues[field] : undefined);
  const nameError = fieldError("name");
  const accountError = fieldError("account");
  const addressError = fieldError("address");
  const mountError = fieldError("mount");
  const namespaceError = fieldError("namespace");
  const tokenError = fieldError("token");
  const regionError = fieldError("region");
  const profileError = fieldError("profile");
  const authOption = AUTH_OPTIONS.find((option) => option.auth === draft.auth);

  const update = (patch: Partial<SecretProviderDraft>) => setDraft({ ...draft, ...patch });
  const close = () => {
    setDraft(EMPTY_SECRET_PROVIDER_DRAFT);
    setSubmitted(false);
    onOpenChange(false);
  };
  const save = () => {
    setSubmitted(true);
    if (Result.isFailure(result)) return;
    updateSettings({ secretProviders: { [result.success.id]: result.success.config } });
    close();
  };

  const kindFields = () => {
    switch (draft.kind) {
      case "1password":
        return (
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
                        <span className="min-w-0 truncate text-sm font-medium">{item.account}</span>
                        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
                          {item.email}
                        </span>
                      </AutocompleteItem>
                    ))}
                  </AutocompleteList>
                </AutocompletePopup>
              ) : null}
            </Autocomplete>
            {accountError !== undefined ? (
              <p
                id="secret-provider-account-error"
                role="alert"
                className="text-xs text-destructive"
              >
                {accountError}
              </p>
            ) : null}
            <p className="text-xs text-muted-foreground">
              Accounts the op CLI is signed into are suggested. Type any other sign-in address.
            </p>
          </div>
        );
      case "openbao":
        return (
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
              {addressError !== undefined ? (
                <p
                  id="secret-provider-address-error"
                  role="alert"
                  className="text-xs text-destructive"
                >
                  {addressError}
                </p>
              ) : null}
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
              {mountError !== undefined ? (
                <p
                  id="secret-provider-mount-error"
                  role="alert"
                  className="text-xs text-destructive"
                >
                  {mountError}
                </p>
              ) : null}
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
              {namespaceError !== undefined ? (
                <p
                  id="secret-provider-namespace-error"
                  role="alert"
                  className="text-xs text-destructive"
                >
                  {namespaceError}
                </p>
              ) : null}
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
                  onChange={(event) => update({ token: event.target.value })}
                  aria-invalid={tokenError !== undefined}
                  aria-describedby={
                    tokenError !== undefined ? "secret-provider-token-error" : undefined
                  }
                />
                {tokenError !== undefined ? (
                  <p
                    id="secret-provider-token-error"
                    role="alert"
                    className="text-xs text-destructive"
                  >
                    {tokenError}
                  </p>
                ) : null}
              </div>
            ) : null}
          </>
        );
      case "aws-secrets-manager":
        return (
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
              {regionError !== undefined ? (
                <p
                  id="secret-provider-region-error"
                  role="alert"
                  className="text-xs text-destructive"
                >
                  {regionError}
                </p>
              ) : null}
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
              {profileError !== undefined ? (
                <p
                  id="secret-provider-profile-error"
                  role="alert"
                  className="text-xs text-destructive"
                >
                  {profileError}
                </p>
              ) : null}
              <p className="text-xs text-muted-foreground">
                Credentials come from the aws CLI configuration on the server: this profile, or its
                default chain when empty.
              </p>
            </div>
          </>
        );
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
        else onOpenChange(next);
      }}
    >
      <DialogPopup className="max-w-md">
        <DialogHeader>
          <DialogTitle>Add a secret provider</DialogTitle>
          <DialogDescription>
            Provider variables on {environmentLabel} can read values from it by name. Values are
            read on that server when a provider starts.
          </DialogDescription>
        </DialogHeader>
        <DialogPanel>
          <form
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
                onValueChange={(kind) => {
                  if (kind !== null) update({ kind });
                }}
              >
                <SelectTrigger id="secret-provider-kind">
                  <SelectValue>
                    <SecretProviderKindLabel kind={draft.kind} />
                  </SelectValue>
                </SelectTrigger>
                <SelectPopup>
                  {KIND_OPTIONS.map((option) => (
                    <SelectItem key={option.kind} value={option.kind}>
                      <SecretProviderKindLabel kind={option.kind} />
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
            {kindFields()}
          </form>
        </DialogPanel>
        <DialogFooter variant="bare">
          <Button variant="outline" onClick={close}>
            Cancel
          </Button>
          <Button onClick={save} disabled={submitted && Result.isFailure(result)}>
            Add provider
          </Button>
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  );
}
