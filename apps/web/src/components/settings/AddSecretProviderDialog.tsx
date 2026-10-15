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
} from "./AddSecretProviderDialog.logic";
import { SecretInput } from "./SecretInput";

const KIND_OPTIONS: ReadonlyArray<{ readonly kind: SecretProviderKind; readonly label: string }> = [
  { kind: "1password", label: "1Password CLI" },
  { kind: "openbao", label: "OpenBao" },
];

/**
 * Adds a named secret provider to one environment. Provider variables on that
 * environment pick it by name; the server runs its CLI when a provider starts.
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
  const issue = Result.isFailure(result) ? result.failure : undefined;

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
            Provider variables on {environmentLabel} can read values from it by name. Its CLI runs
            on that server.
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
                autoFocus
              />
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
                  />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-mount">Mount</Label>
                  <Input
                    id="secret-provider-mount"
                    placeholder={DEFAULT_OPENBAO_MOUNT}
                    value={draft.mount}
                    onChange={(event) => update({ mount: event.target.value })}
                    spellCheck={false}
                  />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-namespace">Namespace (optional)</Label>
                  <Input
                    id="secret-provider-namespace"
                    value={draft.namespace}
                    onChange={(event) => update({ namespace: event.target.value })}
                    spellCheck={false}
                  />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="secret-provider-token">Token (optional)</Label>
                  <SecretInput
                    id="secret-provider-token"
                    value={draft.token}
                    onChange={(event) => update({ token: event.target.value })}
                  />
                  <p className="text-xs text-muted-foreground">
                    Leave empty to use the bao CLI's own login.
                  </p>
                </div>
              </>
            )}
            {submitted && issue !== undefined ? (
              <p role="alert" className="text-xs text-destructive">
                {issue}
              </p>
            ) : null}
          </form>
        </DialogPanel>
        <DialogFooter variant="bare">
          <Button variant="outline" onClick={close}>
            Cancel
          </Button>
          <Button onClick={save} disabled={submitted && issue !== undefined}>
            Add provider
          </Button>
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  );
}
