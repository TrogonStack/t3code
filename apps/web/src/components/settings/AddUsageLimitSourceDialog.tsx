import type { EnvironmentId, UnifiedSettings } from "@t3tools/contracts";
import { useState } from "react";

import { useEnvironmentSettings, useUpdateEnvironmentSettings } from "../../hooks/useSettings";
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
import { sourceIdFromUrl } from "./AddUsageLimitSourceDialog.logic";
import { SecretInput } from "./SecretInput";
import { Label } from "../ui/label";
import {
  EMPTY_SECRET_FIELD_DRAFT,
  SecretField,
  secretFieldSubmission,
} from "./SecretSourceControls";

/** Send the management key once to server secret storage; settings retain only a redaction marker. */
export function AddUsageLimitSourceDialog({
  open,
  onOpenChange,
  environmentId,
  environmentLabel,
  sources,
}: {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly environmentId: EnvironmentId;
  readonly environmentLabel: string;
  readonly sources: UnifiedSettings["usageLimitSources"];
}) {
  const updateSettings = useUpdateEnvironmentSettings(environmentId);
  const [label, setLabel] = useState("");
  const [url, setUrl] = useState("");
  const secretProviders = useEnvironmentSettings(
    environmentId,
    (settings) => settings.secretProviders,
  );
  const [keyDraft, setKeyDraft] = useState(EMPTY_SECRET_FIELD_DRAFT);
  const managementKey = Result.getOrUndefined(secretFieldSubmission(keyDraft, secretProviders));
  const trimmedUrl = url.trim();
  const canSave = trimmedUrl.length > 0 && managementKey !== undefined;

  const reset = () => {
    setLabel("");
    setUrl("");
    setKeyDraft(EMPTY_SECRET_FIELD_DRAFT);
  };

  const save = () => {
    if (!canSave) return;
    const id = sourceIdFromUrl(trimmedUrl, sources);
    updateSettings({
      usageLimitSources: {
        [id]: {
          kind: "cliproxy",
          ...(label.trim() ? { label: label.trim() } : {}),
          url: trimmedUrl,
          managementKey,
          enabled: true,
        },
      },
    });
    reset();
    onOpenChange(false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) reset();
        onOpenChange(next);
      }}
    >
      <DialogPopup className="max-w-md">
        <DialogHeader>
          <DialogTitle>Add a CLIProxyAPI hub</DialogTitle>
          <DialogDescription>
            Show the quota of every account the hub pools, next to the providers on{" "}
            {environmentLabel}. The key stays on that server.
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
              <Label htmlFor="usage-source-url">Hub URL</Label>
              <Input
                id="usage-source-url"
                placeholder="https://hub.example.ts.net:8318"
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                autoFocus
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="usage-source-key">Management key</Label>
              <SecretField
                id="usage-source-key"
                label="Management key"
                environmentId={environmentId}
                secretProviders={secretProviders}
                value={undefined}
                draft={keyDraft}
                onDraftChange={setKeyDraft}
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="usage-source-label">Label (optional)</Label>
              <Input
                id="usage-source-label"
                placeholder="Defaults to the hub's host name"
                value={label}
                onChange={(event) => setLabel(event.target.value)}
              />
            </div>
          </form>
        </DialogPanel>
        <DialogFooter variant="bare">
          <Button
            variant="outline"
            onClick={() => {
              reset();
              onOpenChange(false);
            }}
          >
            Cancel
          </Button>
          <Button onClick={save} disabled={!canSave}>
            Add hub
          </Button>
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  );
}
