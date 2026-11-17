import {
  AuthProvidersManageScope,
  AuthSettingsWriteScope,
  isManagedDriverAllowed,
  type EnvironmentId,
  type ProjectId,
  type ProviderProfile,
  type ServerProvider,
  type ServerSettingsPatch,
} from "@t3tools/contracts";
import {
  isAtomCommandInterrupted,
  squashAtomCommandFailure,
} from "@t3tools/client-runtime/state/runtime";
import { useRef, useState } from "react";
import { View } from "react-native";

import { AppText as Text, AppTextInput } from "../../components/ui/AppText";
import { Button } from "../../components/ui/Button";
import * as Dialog from "../../components/ui/Dialog";
import { ListChoiceRow } from "../../components/ui/ListChoiceRow";
import { uuidv4 } from "../../lib/uuid";
import { serverEnvironment } from "../../state/server";
import { readEnvironmentScope, useEnvironmentScope } from "../../state/session";
import { useAtomCommand } from "../../state/use-atom-command";
import type { SettingsTarget } from "./settings-environment-filter";
import { SettingsSection } from "./components/SettingsSection";
import { SettingsSwitchRow } from "./components/SettingsSwitchRow";

function useProfileSettingsWrite(environmentId: EnvironmentId) {
  const [pending, setPending] = useState(false);
  const pendingRef = useRef(false);
  const updateSettings = useAtomCommand(serverEnvironment.updateSettings, {
    label: "provider profile update",
    reportFailure: true,
  });
  const write = async (patch: ServerSettingsPatch): Promise<boolean> => {
    if (
      pendingRef.current ||
      !readEnvironmentScope(environmentId, AuthSettingsWriteScope) ||
      !readEnvironmentScope(environmentId, AuthProvidersManageScope)
    ) {
      return false;
    }
    pendingRef.current = true;
    setPending(true);
    const result = await updateSettings({ environmentId, input: { patch } });
    pendingRef.current = false;
    setPending(false);
    if (result._tag === "Failure") {
      if (!isAtomCommandInterrupted(result)) {
        const error = squashAtomCommandFailure(result);
        Dialog.alert("Could not save provider profile", String(error));
      }
      return false;
    }
    return true;
  };
  return { write, pending };
}

export function ProjectProviderProfileSection(props: {
  readonly environment: SettingsTarget | undefined;
  readonly projectId: ProjectId;
  readonly checkoutLabel: string;
  readonly showCheckoutLabel: boolean;
}) {
  if (!props.environment) {
    return (
      <SettingsSection title="Provider profile">
        <Text className="p-4 text-sm text-foreground-muted">
          Connect this checkout to view its provider profile.
        </Text>
      </SettingsSection>
    );
  }
  return <ConnectedProjectProviderProfileSection {...props} environment={props.environment} />;
}

function ConnectedProjectProviderProfileSection(props: {
  readonly environment: SettingsTarget;
  readonly projectId: ProjectId;
  readonly checkoutLabel: string;
  readonly showCheckoutLabel: boolean;
}) {
  const { environment, projectId } = props;
  const allowed = useEnvironmentScope(environment.environmentId, AuthSettingsWriteScope);
  const canManageProviders = useEnvironmentScope(
    environment.environmentId,
    AuthProvidersManageScope,
  );
  const { write, pending } = useProfileSettingsWrite(environment.environmentId);
  const { providerProfiles, projectProfiles } = environment.serverConfig.settings.trogonStack;
  const selectedId = projectProfiles[projectId] ?? null;
  const needsProfile =
    selectedId === null &&
    environment.serverConfig.settings.trogonStack.projectsRequiringProfile?.[projectId] === true;
  const choices = Object.entries(providerProfiles).sort((left, right) =>
    left[1].name.localeCompare(right[1].name),
  );
  const select = (profileId: string | null) => {
    const save = () => {
      void write({ trogonStack: { projectProfiles: { [projectId]: profileId } } });
    };
    if (profileId === selectedId) return;
    if (selectedId === null && profileId !== null) {
      save();
      return;
    }
    Dialog.confirm({
      title: profileId === null ? "Remove provider profile?" : "Change provider profile?",
      message:
        profileId === null
          ? needsProfile
            ? "Removing the assignment blocks providers until another profile is assigned. Existing thread context is not erased."
            : "Removing the assignment returns this project to legacy, unrestricted provider access. Existing thread context is not erased."
          : "Changing this assignment does not remove existing thread context.",
      confirmText: profileId === null ? "Remove profile" : "Change profile",
      onConfirm: save,
    });
  };

  return (
    <SettingsSection title="Provider profile">
      <View className="gap-1 p-4">
        {props.showCheckoutLabel ? (
          <Text className="font-t3-medium text-base text-foreground">{props.checkoutLabel}</Text>
        ) : null}
        <Text className="text-sm text-foreground-muted">
          {selectedId === null
            ? needsProfile
              ? "Profile required: providers are blocked until you assign one."
              : "Legacy project: providers are not restricted by a profile."
            : providerProfiles[selectedId]
              ? "Only the selected profile's provider instances are available for future turns. Existing thread context is unchanged."
              : "Assigned profile is missing. No provider instances are allowed until you select a profile."}
        </Text>
      </View>
      <ListChoiceRow
        label={
          needsProfile ? "No profile (providers blocked)" : "No profile (legacy, unrestricted)"
        }
        selected={selectedId === null}
        disabled={!allowed || !canManageProviders || pending}
        onPress={() => select(null)}
      />
      {choices.map(([id, profile]) => (
        <ListChoiceRow
          key={id}
          label={profile.name}
          description={`${profile.providerInstanceIds.length} provider instances`}
          selected={selectedId === id}
          disabled={!allowed || !canManageProviders || pending}
          separated
          onPress={() => select(id)}
        />
      ))}
      {choices.length === 0 ? (
        <Text className="p-4 text-sm text-foreground-muted">
          Add a provider profile in Provider settings before assigning this project.
        </Text>
      ) : null}
    </SettingsSection>
  );
}

export function ProviderProfilesSection(props: {
  readonly environment: SettingsTarget;
  readonly providers: ReadonlyArray<ServerProvider>;
}) {
  const { environment } = props;
  const [name, setName] = useState("");
  const allowed = useEnvironmentScope(environment.environmentId, AuthSettingsWriteScope);
  const canManageProviders = useEnvironmentScope(
    environment.environmentId,
    AuthProvidersManageScope,
  );
  const { write, pending } = useProfileSettingsWrite(environment.environmentId);
  const profiles = environment.serverConfig.settings.trogonStack.providerProfiles;
  const create = async () => {
    const nextName = name.trim();
    if (
      !nextName ||
      Object.values(profiles).some(
        (profile) => profile.name.toLowerCase() === nextName.toLowerCase(),
      )
    ) {
      Dialog.alert("Choose a unique profile name");
      return;
    }
    // A reused id would silently rebind projects still pointing at a deleted profile.
    const profileId = uuidv4();
    const saved = await write({
      trogonStack: {
        providerProfiles: { [profileId]: { name: nextName, providerInstanceIds: [] } },
      },
    });
    if (saved) setName("");
  };

  return (
    <SettingsSection title={`Provider profiles · ${environment.label}`}>
      <View className="gap-2 p-4">
        <Text className="text-sm text-foreground-muted">
          Profiles allow only selected provider instances. New providers stay off until added to a
          profile.
        </Text>
        <AppTextInput
          accessibilityLabel="New provider profile name"
          value={name}
          onChangeText={setName}
          placeholder="Profile name"
          editable={allowed && canManageProviders && !pending}
          returnKeyType="done"
          onSubmitEditing={() => void create()}
        />
        <Button
          label="Add profile"
          tone="secondary"
          disabled={!allowed || !canManageProviders || pending || !name.trim()}
          onPress={() => void create()}
        />
        {!allowed || !canManageProviders ? (
          <Text className="text-sm text-foreground-muted">
            This connection cannot edit server settings.
          </Text>
        ) : null}
      </View>
      {Object.entries(profiles).map(([id, profile]) => (
        <ProviderProfileEditor
          key={id}
          environment={environment}
          providers={props.providers}
          id={id}
          profile={profile}
        />
      ))}
    </SettingsSection>
  );
}

function ProviderProfileEditor(props: {
  readonly environment: SettingsTarget;
  readonly providers: ReadonlyArray<ServerProvider>;
  readonly id: string;
  readonly profile: ProviderProfile;
}) {
  const { environment, id, profile } = props;
  const [draftName, setDraftName] = useState<string | null>(null);
  const allowed = useEnvironmentScope(environment.environmentId, AuthSettingsWriteScope);
  const canManageProviders = useEnvironmentScope(
    environment.environmentId,
    AuthProvidersManageScope,
  );
  const { write, pending } = useProfileSettingsWrite(environment.environmentId);
  const profiles = environment.serverConfig.settings.trogonStack;
  const assigned = Object.values(profiles.projectProfiles).filter(
    (assignedId) => assignedId === id,
  ).length;
  const save = (next: ProviderProfile) =>
    write({ trogonStack: { providerProfiles: { [id]: next } } });
  const rename = async () => {
    const nextName = (draftName ?? profile.name).trim();
    if (
      !nextName ||
      Object.entries(profiles.providerProfiles).some(
        ([otherId, other]) => otherId !== id && other.name.toLowerCase() === nextName.toLowerCase(),
      )
    ) {
      Dialog.alert("Choose a unique profile name");
      return;
    }
    if (await save({ ...profile, name: nextName })) setDraftName(null);
  };
  const remove = () => {
    if (assigned) return;
    Dialog.confirm({
      title: `Delete ${profile.name}?`,
      message: "This profile is not assigned to any projects.",
      confirmText: "Delete profile",
      destructive: true,
      onConfirm: () => {
        void write({ trogonStack: { providerProfiles: { [id]: null } } });
      },
    });
  };

  return (
    <View className="border-t border-border-subtle">
      <View className="gap-2 p-4">
        <AppTextInput
          accessibilityLabel={`Name for provider profile ${profile.name}`}
          value={draftName ?? profile.name}
          onChangeText={setDraftName}
          editable={allowed && canManageProviders && !pending}
          onSubmitEditing={() => void rename()}
          returnKeyType="done"
        />
        <View className="flex-row flex-wrap gap-2">
          <Button
            label="Save name"
            tone="secondary"
            disabled={
              !allowed ||
              !canManageProviders ||
              pending ||
              draftName === null ||
              draftName.trim() === profile.name
            }
            onPress={() => void rename()}
          />
          <Button
            label="Delete profile"
            tone="text"
            disabled={!allowed || !canManageProviders || pending || assigned > 0}
            onPress={remove}
          />
        </View>
        {assigned > 0 ? (
          <Text className="text-sm text-foreground-muted">
            Assigned to {assigned} projects. Reassign them before deleting this profile.
          </Text>
        ) : null}
      </View>
      {props.providers.map((provider) => (
        <SettingsSwitchRow
          key={provider.instanceId}
          label={provider.displayName ?? provider.instanceId}
          subtitle={provider.displayName ? provider.instanceId : provider.driver}
          value={profile.providerInstanceIds.includes(provider.instanceId)}
          disabled={
            !allowed ||
            !canManageProviders ||
            pending ||
            !isManagedDriverAllowed(environment.serverConfig.managedPolicy, provider.driver)
          }
          onValueChange={(enabled) => {
            const providerInstanceIds = enabled
              ? [...profile.providerInstanceIds, provider.instanceId]
              : profile.providerInstanceIds.filter(
                  (instanceId) => instanceId !== provider.instanceId,
                );
            void save({ ...profile, providerInstanceIds });
          }}
        />
      ))}
    </View>
  );
}
