import type {
  FlatSecretProviderKind,
  SecretProviderId,
  StoredSecretProviderConfig,
} from "@t3tools/contracts";
import { PlusIcon } from "lucide-react";
import { useState } from "react";

import { useEnvironmentSettings, useUpdateEnvironmentSettings } from "../../hooks/useSettings";
import type { EnvironmentPresentation } from "../../state/environments";
import { useEnvironmentSessionState } from "../../state/session";
import {
  AlertDialog,
  AlertDialogClose,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogPopup,
  AlertDialogTitle,
} from "../ui/alert-dialog";
import { Button } from "../ui/button";
import { SecretProviderDialog } from "./SecretProviderDialog";
import {
  classifySecretProvider,
  secretProviderDisplayKind,
  secretProviderSummary,
} from "./SecretProviderDialog.logic";
import {
  classifyProviderEnvironmentAccess,
  resolveRemoteOperateAccess,
} from "./ProviderSettingsPanel.logic";
import { useSettingsScope } from "./SettingsScopeContext";
import { RedactedSensitiveText } from "./RedactedSensitiveText";
import { SecretProviderKindIcon } from "./SecretProviderKindIcon";
import { searchableSetting } from "./settingsSearch";
import { SettingsPageContainer, SettingsRow, SettingsSection } from "./settingsLayout";

function secretProviderDescription(provider: StoredSecretProviderConfig): string {
  const classified = classifySecretProvider(provider);
  if (classified.kind !== "flat") return secretProviderSummary(provider);
  const config = classified.config;
  switch (config.kind) {
    case "1password":
      return secretProviderSummary(config);
    case "openbao":
      return [
        secretProviderSummary(config),
        `mount ${config.mount}`,
        ...(config.namespace === undefined ? [] : [`namespace ${config.namespace}`]),
        ...(config.auth === "env" ? ["BAO_TOKEN"] : config.auth === "cli" ? ["bao CLI login"] : []),
      ].join(" · ");
    case "aws-secrets-manager":
      return [
        secretProviderSummary(config),
        ...(config.profile === undefined ? [] : [`profile ${config.profile}`]),
      ].join(" · ");
  }
}

export function SecretsSettingsPanel() {
  const { scope, environment } = useSettingsScope();
  const connected = environment?.connection.phase === "connected";
  return (
    <SettingsPageContainer>
      {connected &&
      environment.serverConfig !== null &&
      environment.serverConfig.secretProviderSettings !== true ? (
        <SettingsSection {...searchableSetting("secret-providers")}>
          <p className="px-4 py-3 text-sm text-muted-foreground">
            Update T3 Code on {environment.label} to manage its secret providers.
          </p>
        </SettingsSection>
      ) : connected ? (
        <EnvironmentSecretProviders
          key={environment.environmentId}
          environment={environment}
          projectScoped={scope.kind === "project" || scope.kind === "checkout"}
        />
      ) : (
        <SettingsSection {...searchableSetting("secret-providers")}>
          <p className="px-4 py-3 text-sm text-muted-foreground">
            Connect an environment to manage the secret providers its provider variables read from.
          </p>
        </SettingsSection>
      )}
    </SettingsPageContainer>
  );
}

function EnvironmentSecretProviders({
  environment,
  projectScoped,
}: {
  readonly environment: EnvironmentPresentation;
  readonly projectScoped: boolean;
}) {
  const environmentId = environment.environmentId;
  const sessionState = useEnvironmentSessionState(environmentId);
  // The server requires the same grant for secret providers as for provider settings.
  const access = classifyProviderEnvironmentAccess({
    connectionPhase: environment.connection.phase,
    hasServerConfig: environment.serverConfig !== null,
    operateAccess: resolveRemoteOperateAccess({
      session: sessionState.data,
      isPending: sessionState.isPending,
      hasError: sessionState.hasError,
    }),
  });
  const readOnly = access.kind !== "editable" || projectScoped;
  const providers = useEnvironmentSettings(environmentId, (settings) => settings.secretProviders);
  const updateSettings = useUpdateEnvironmentSettings(environmentId);
  const [dialog, setDialog] = useState<
    null | { readonly mode: "add" } | { readonly mode: "edit"; readonly id: SecretProviderId }
  >(null);
  const entries = Object.entries(providers) as Array<
    [SecretProviderId, StoredSecretProviderConfig]
  >;
  const editingConfig = dialog?.mode === "edit" ? providers[dialog.id] : undefined;
  const editing =
    dialog?.mode === "edit" && editingConfig !== undefined
      ? { id: dialog.id, config: editingConfig }
      : undefined;
  return (
    <>
      <SettingsSection
        {...searchableSetting("secret-providers")}
        headerAction={
          !readOnly ? (
            <Button size="xs" variant="outline" onClick={() => setDialog({ mode: "add" })}>
              <PlusIcon className="size-3" aria-hidden />
              Add provider
            </Button>
          ) : null
        }
      >
        {projectScoped ? (
          <p className="px-4 pt-3 text-xs text-muted-foreground">
            Environment-wide setting. Select an environment to change it.
          </p>
        ) : null}
        {entries.length === 0 ? (
          <SettingsRow
            title="No secret providers configured."
            description="Add 1Password, OpenBao, or AWS Secrets Manager, then pick it as the source of a provider variable."
          />
        ) : (
          entries.map(([id, config]) => {
            const kind = secretProviderDisplayKind(config);
            const classified = classifySecretProvider(config);
            return (
              <SettingsRow
                key={id}
                title={<SecretProviderTitle kind={kind} label={config.label} />}
                description={
                  classified.kind === "one_password" ? (
                    <span className="flex min-w-0 items-center gap-1">
                      <span className="shrink-0">1Password ·</span>
                      <RedactedSensitiveText
                        value={classified.account}
                        ariaLabel="Toggle account visibility"
                        revealTooltip="Click to reveal account"
                        hideTooltip="Click to hide account"
                        className="max-w-full truncate"
                      />
                    </span>
                  ) : classified.kind === "unsupported" ? (
                    `Unsupported provider (${config.kind})`
                  ) : (
                    <span className="break-all">{secretProviderDescription(config)}</span>
                  )
                }
                control={
                  !readOnly ? (
                    <div className="flex items-center gap-1">
                      {kind !== undefined ? (
                        <Button
                          size="xs"
                          variant="ghost"
                          onClick={() => setDialog({ mode: "edit", id })}
                        >
                          Edit
                        </Button>
                      ) : null}
                      <RemoveSecretProviderButton
                        label={config.label}
                        onConfirm={() => updateSettings({ secretProviders: { [id]: null } })}
                      />
                    </div>
                  ) : null
                }
              />
            );
          })
        )}
      </SettingsSection>
      {dialog !== null && !readOnly && (dialog.mode === "add" || editing !== undefined) ? (
        <SecretProviderDialog
          open
          onOpenChange={(open) => {
            if (!open) setDialog(null);
          }}
          environmentId={environmentId}
          environmentLabel={environment.label}
          existingIds={new Set(Object.keys(providers))}
          {...(editing === undefined ? {} : { existing: editing })}
        />
      ) : null}
    </>
  );
}

function SecretProviderTitle({
  kind,
  label,
}: {
  readonly kind: FlatSecretProviderKind | undefined;
  readonly label: string;
}) {
  return (
    <span className="flex min-w-0 items-center gap-2">
      {kind !== undefined ? (
        <SecretProviderKindIcon
          kind={kind}
          className="size-4.5 shrink-0 text-foreground/80"
          aria-hidden
        />
      ) : null}
      <span className="min-w-0 wrap-anywhere">{label}</span>
    </span>
  );
}

function RemoveSecretProviderButton({
  label,
  onConfirm,
}: {
  readonly label: string;
  readonly onConfirm: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button size="xs" variant="ghost" onClick={() => setOpen(true)}>
        Remove
      </Button>
      <AlertDialog open={open} onOpenChange={setOpen}>
        <AlertDialogPopup>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove {label}?</AlertDialogTitle>
            <AlertDialogDescription>
              Provider variables that read from {label} stop resolving until a provider with the
              same name is added again. The secrets themselves are untouched.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogClose render={<Button variant="outline" />}>Cancel</AlertDialogClose>
            <Button
              variant="destructive"
              onClick={() => {
                setOpen(false);
                onConfirm();
              }}
            >
              Remove provider
            </Button>
          </AlertDialogFooter>
        </AlertDialogPopup>
      </AlertDialog>
    </>
  );
}
