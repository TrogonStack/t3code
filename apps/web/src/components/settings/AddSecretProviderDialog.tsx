import type { EnvironmentId, SecretProviderKind } from "@t3tools/contracts";
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
import { Checkbox } from "../ui/checkbox";
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

const KIND_OPTIONS: ReadonlyArray<{ readonly kind: SecretProviderKind; readonly label: string }> = [
  { kind: "1password", label: "1Password CLI" },
  { kind: "openbao", label: "OpenBao" },
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
                    {KIND_OPTIONS.find((option) => option.kind === draft.kind)?.label}
                  </SelectValue>
                </SelectTrigger>
                <SelectPopup>
                  {KIND_OPTIONS.map((option) => (
                    <SelectItem key={option.kind} value={option.kind}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectPopup>
              </Select>
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="secret-provider-name">Name</Label>
              <Input
                id="secret-provider-name"
                placeholder={draft.kind === "1password" ? "Personal" : "Homelab"}
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
            {draft.kind === "1password" ? (
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
            ) : (
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
                  <Label htmlFor="secret-provider-token">Token (optional)</Label>
                  <SecretInput
                    id="secret-provider-token"
                    placeholder="s.xxxxxxxxxxxxxxxxxxxxxxxx"
                    value={draft.token}
                    onChange={(event) => update({ token: event.target.value })}
                  />
                  <p className="text-xs text-muted-foreground">
                    {draft.access === "cli"
                      ? "Leave empty to use the bao CLI's own login."
                      : "Leave empty to use BAO_TOKEN or the token saved by bao login on that server."}
                  </p>
                </div>
                <label className="flex cursor-pointer items-start gap-3">
                  <Checkbox
                    className="mt-0.5"
                    checked={draft.access === "cli"}
                    onCheckedChange={(checked) =>
                      update({ access: checked === true ? "cli" : "api" })
                    }
                  />
                  <span className="min-w-0">
                    <span className="block text-sm font-medium text-foreground">
                      Use the bao CLI
                    </span>
                    <span className="block text-xs leading-snug text-muted-foreground">
                      Run the bao command on the server instead of calling the HTTP API.
                    </span>
                  </span>
                </label>
              </>
            )}
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
