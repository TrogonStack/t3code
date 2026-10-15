import type { SecretProviderConfig, SecretProviderId } from "@t3tools/contracts";
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
import { AddSecretProviderDialog } from "./AddSecretProviderDialog";
import { secretProviderSummary } from "./AddSecretProviderDialog.logic";
import {
  classifyProviderEnvironmentAccess,
  resolveRemoteOperateAccess,
} from "./ProviderSettingsPanel.logic";
import { useSettingsScope } from "./SettingsScopeContext";
import { searchableSetting } from "./settingsSearch";
import { SettingsPageContainer, SettingsRow, SettingsSection } from "./settingsLayout";

function secretProviderDescription(config: SecretProviderConfig): string {
  if (config.kind === "1password") return secretProviderSummary(config);
  return [
    secretProviderSummary(config),
    `mount ${config.mount}`,
    ...(config.namespace === undefined ? [] : [`namespace ${config.namespace}`]),
    ...(config.access === "cli" ? ["bao CLI"] : []),
  ].join(" · ");
}

/** Secret providers belong to one environment, so this page shows the representative one. */
export function SecretsSettingsPanel() {
  const { scope, environment, connectedEnvironments } = useSettingsScope();
  const aggregate = scope.environmentIds.length !== 1 && connectedEnvironments.length > 1;
  return (
    <SettingsPageContainer>
      {environment?.connection.phase === "connected" ? (
        <EnvironmentSecretProviders
          key={environment.environmentId}
          environment={environment}
          titleSuffix={aggregate ? ` · ${environment.label}` : ""}
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
  titleSuffix,
}: {
  readonly environment: EnvironmentPresentation;
  readonly titleSuffix: string;
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
  const readOnly = access.kind !== "editable";
  const providers = useEnvironmentSettings(environmentId, (settings) => settings.secretProviders);
  const updateSettings = useUpdateEnvironmentSettings(environmentId);
  const [adding, setAdding] = useState(false);
  const entries = Object.entries(providers) as Array<[SecretProviderId, SecretProviderConfig]>;
  const section = searchableSetting("secret-providers");

  return (
    <>
      <SettingsSection
        id={section.id}
        title={`${section.title}${titleSuffix}`}
        headerAction={
          !readOnly ? (
            <Button size="xs" variant="outline" onClick={() => setAdding(true)}>
              <PlusIcon className="size-3" aria-hidden />
              Add provider
            </Button>
          ) : null
        }
      >
        {entries.length === 0 ? (
          <SettingsRow
            title="No secret providers configured."
            description="Add 1Password or OpenBao, then pick it as the source of a provider variable."
          />
        ) : (
          entries.map(([id, config]) => (
            <SettingsRow
              key={id}
              title={config.label}
              description={<span className="break-all">{secretProviderDescription(config)}</span>}
              control={
                !readOnly ? (
                  <RemoveSecretProviderButton
                    label={config.label}
                    onConfirm={() => updateSettings({ secretProviders: { [id]: null } })}
                  />
                ) : null
              }
            />
          ))
        )}
      </SettingsSection>
      {adding && !readOnly ? (
        <AddSecretProviderDialog
          open
          onOpenChange={setAdding}
          environmentId={environmentId}
          environmentLabel={environment.label}
          existingIds={new Set(Object.keys(providers))}
        />
      ) : null}
    </>
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
