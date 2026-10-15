import type { EnvironmentId, SecretProviderKind } from "@t3tools/contracts";
import * as Result from "effect/Result";
import { useState } from "react";

import { useUpdateEnvironmentSettings } from "../../hooks/useSettings";
import { useEnvironmentQuery } from "../../state/query";
import { serverEnvironment } from "../../state/server";
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
  // Without any account from the server there is nothing to pick, so it is typed.
  const picksAccount = listedAccounts.length > 0;
  const effectiveDraft: SecretProviderDraft =
    picksAccount && draft.account.length === 0
      ? { ...draft, account: listedAccounts[0]?.account ?? "" }
      : draft;
  const result = secretProviderFromDraft(effectiveDraft, existingIds);
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
                {picksAccount ? (
                  <Select
                    value={effectiveDraft.account}
                    onValueChange={(account) => {
                      if (account !== null) update({ account });
                    }}
                  >
                    <SelectTrigger id="secret-provider-account">
                      <SelectValue>{effectiveDraft.account}</SelectValue>
                    </SelectTrigger>
                    <SelectPopup>
                      {listedAccounts.map(({ account, email }) => (
                        <SelectItem key={account} value={account}>
                          <span className="flex min-w-0 flex-col">
                            <span className="truncate">{account}</span>
                            <span className="truncate text-xs text-muted-foreground">{email}</span>
                          </span>
                        </SelectItem>
                      ))}
                    </SelectPopup>
                  </Select>
                ) : (
                  <Input
                    id="secret-provider-account"
                    placeholder="my.1password.com"
                    value={draft.account}
                    onChange={(event) => update({ account: event.target.value })}
                    spellCheck={false}
                  />
                )}
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
