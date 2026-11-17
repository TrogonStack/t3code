import {
  AuthProvidersManageScope,
  AuthSettingsWriteScope,
  type EnvironmentId,
  type ProjectId,
  type ProviderInstanceId,
  type ProviderProfile,
  type ServerSettings,
  type ServerSettingsPatch,
} from "@t3tools/contracts";
import { PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";

import { useEnvironmentSettings, useUpdateEnvironmentSettings } from "../../hooks/useSettings";
import { randomUUID } from "../../lib/utils";
import { useEnvironmentScope } from "../../state/session";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Input } from "../ui/input";
import { Select, SelectItem, SelectPopup, SelectTrigger, SelectValue } from "../ui/select";
import { ensureLocalApi } from "../../localApi";
import {
  profileAssignmentLabel,
  projectRequiresProfile,
  removeProfilePatch,
  setProfilePatch,
  setProjectProfilePatch,
  toggleProfileInstance,
} from "./providerProfiles.logic";
import { SettingsRow, SettingsSection } from "./settingsLayout";

type SettingsWriter = (
  patch: ServerSettingsPatch,
  options?: { readonly onSaved?: () => void; readonly onFailed?: () => void },
) => void;

const NO_PROFILE = "__no_provider_profile__";

function ProfileEditor({
  profileId,
  profile,
  settings,
  availableInstances,
  disabled,
  onPatch,
}: {
  profileId: string;
  profile: ProviderProfile;
  settings: ServerSettings;
  availableInstances: ReadonlyArray<{ instanceId: ProviderInstanceId; label: string }>;
  disabled: boolean;
  onPatch: SettingsWriter;
}) {
  const [saving, setSaving] = useState(false);
  const save = (patch: ServerSettingsPatch) => {
    setSaving(true);
    onPatch(patch, { onSaved: () => setSaving(false), onFailed: () => setSaving(false) });
  };
  const assignedProjects = Object.values(settings.trogonStack.projectProfiles).filter(
    (assignedId) => assignedId === profileId,
  ).length;
  const knownIds = new Set(availableInstances.map(({ instanceId }) => instanceId));
  const instances = [
    ...availableInstances,
    ...profile.providerInstanceIds
      .filter((instanceId) => !knownIds.has(instanceId))
      .map((instanceId) => ({ instanceId, label: `${instanceId} (not in Providers)` })),
  ];
  const remove = async () => {
    if (assignedProjects > 0) return;
    const confirmed = await ensureLocalApi().dialogs.confirm(
      `Delete the ${profile.name} provider profile?`,
      { variant: "destructive" },
    );
    if (confirmed) save(removeProfilePatch(profileId));
  };

  return (
    <SettingsRow
      title={profile.name}
      description={`${assignedProjects} assigned project${assignedProjects === 1 ? "" : "s"}. Only checked provider instances are allowed for assigned projects.`}
      control={
        <Button
          size="icon-sm"
          variant="destructive-outline"
          aria-label={`Delete provider profile ${profile.name}`}
          disabled={disabled || saving || assignedProjects > 0}
          onClick={() => void remove()}
        >
          <Trash2Icon />
        </Button>
      }
    >
      <div className="flex flex-col gap-3 pb-3">
        <Input
          key={`${profileId}:${profile.name}`}
          size="sm"
          aria-label={`Rename provider profile ${profile.name}`}
          defaultValue={profile.name}
          disabled={disabled || saving}
          onBlur={(event) => {
            const name = event.currentTarget.value.trim();
            if (name && name !== profile.name)
              save(setProfilePatch(profileId, { ...profile, name }));
            else event.currentTarget.value = profile.name;
          }}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing || event.keyCode === 229) return;
            if (event.key === "Enter") event.currentTarget.blur();
          }}
        />
        {instances.length ? (
          <div className="flex flex-wrap gap-x-5 gap-y-3">
            {instances.map(({ instanceId, label }) => (
              <label key={instanceId} className="flex cursor-pointer items-center gap-2 text-sm">
                <Checkbox
                  aria-label={`${profile.name}: ${label}`}
                  checked={profile.providerInstanceIds.includes(instanceId)}
                  disabled={disabled || saving}
                  onCheckedChange={(checked) =>
                    save(
                      setProfilePatch(
                        profileId,
                        toggleProfileInstance(profile, instanceId, checked === true),
                      ),
                    )
                  }
                />
                {label}
              </label>
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">Add a provider instance to allow it here.</p>
        )}
      </div>
    </SettingsRow>
  );
}

export function ProviderProfilesSettings({
  settings,
  instances,
  disabled,
  onPatch,
}: {
  settings: ServerSettings;
  /** The instances the Providers list shows, so a profile offers the same ones. */
  instances: ReadonlyArray<{ instanceId: ProviderInstanceId; label: string }>;
  disabled: boolean;
  onPatch: SettingsWriter;
}) {
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  const profiles = Object.entries(settings.trogonStack.providerProfiles).toSorted((left, right) =>
    left[1].name.localeCompare(right[1].name),
  );
  const create = () => {
    const name = newName.trim();
    if (!name || creating) return;
    setCreating(true);
    onPatch(setProfilePatch(randomUUID(), { name, providerInstanceIds: [] }), {
      onSaved: () => {
        setNewName("");
        setCreating(false);
      },
      onFailed: () => setCreating(false),
    });
  };

  return (
    <SettingsSection id="provider-profiles" title="Provider profiles">
      <SettingsRow
        title="New profile"
        description="Choose allowed instances per profile. New providers are not added automatically. New projects need a profile; existing unassigned projects keep legacy access."
        control={
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <Input
              size="sm"
              aria-label="New provider profile name"
              placeholder="Personal, Company A…"
              value={newName}
              disabled={disabled || creating}
              onChange={(event) => setNewName(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.nativeEvent.isComposing || event.keyCode === 229) return;
                if (event.key === "Enter") create();
              }}
            />
            <Button
              size="sm"
              variant="outline"
              disabled={disabled || creating || !newName.trim()}
              onClick={create}
            >
              <PlusIcon /> Create
            </Button>
          </div>
        }
      />
      {profiles.map(([profileId, profile]) => (
        <ProfileEditor
          key={profileId}
          profileId={profileId}
          profile={profile}
          settings={settings}
          availableInstances={instances}
          disabled={disabled}
          onPatch={onPatch}
        />
      ))}
    </SettingsSection>
  );
}

function ProviderProfileAssignment({
  settings,
  projectId,
  label,
  disabled,
  onPatch,
}: {
  settings: ServerSettings;
  projectId: ProjectId;
  label?: string;
  disabled: boolean;
  onPatch: SettingsWriter;
}) {
  const [saving, setSaving] = useState(false);
  const profileId = settings.trogonStack.projectProfiles[projectId];
  const profiles = Object.entries(settings.trogonStack.providerProfiles).toSorted((left, right) =>
    left[1].name.localeCompare(right[1].name),
  );
  const hasProfile = profileId !== undefined && settings.trogonStack.providerProfiles[profileId];
  const status = profileAssignmentLabel(settings, projectId);
  return (
    <SettingsRow
      title={label ? `Provider profile · ${label}` : "Provider profile"}
      description="Restricts this project's provider instances. A project created while profiles exist cannot run providers until assigned."
      status={!hasProfile ? status : undefined}
      control={
        <div className="w-full sm:w-60">
          <Select
            value={hasProfile ? profileId : NO_PROFILE}
            disabled={disabled || saving}
            onValueChange={(value) => {
              if (value === null) return;
              setSaving(true);
              onPatch(setProjectProfilePatch(projectId, value === NO_PROFILE ? null : value), {
                onSaved: () => setSaving(false),
                onFailed: () => setSaving(false),
              });
            }}
          >
            <SelectTrigger
              size="sm"
              aria-label={label ? `Provider profile for ${label}` : "Provider profile"}
            >
              <SelectValue>{status}</SelectValue>
            </SelectTrigger>
            <SelectPopup>
              <SelectItem value={NO_PROFILE}>
                {projectRequiresProfile(settings, projectId)
                  ? "No profile (providers blocked)"
                  : "No profile (legacy access)"}
              </SelectItem>
              {profiles.map(([id, profile]) => (
                <SelectItem key={id} value={id}>
                  {profile.name}
                </SelectItem>
              ))}
            </SelectPopup>
          </Select>
        </div>
      }
    />
  );
}

export function ProjectProviderProfileAssignment({
  environmentId,
  projectId,
  label,
  available = true,
}: {
  environmentId: EnvironmentId;
  projectId: ProjectId;
  label?: string;
  available?: boolean;
}) {
  const settings = useEnvironmentSettings(environmentId);
  const canWriteSettings = useEnvironmentScope(environmentId, AuthSettingsWriteScope);
  const canManageProviders = useEnvironmentScope(environmentId, AuthProvidersManageScope);
  const updateSettings = useUpdateEnvironmentSettings(environmentId);
  return (
    <ProviderProfileAssignment
      settings={settings}
      projectId={projectId}
      {...(label ? { label } : {})}
      disabled={!available || !canWriteSettings || !canManageProviders}
      onPatch={updateSettings}
    />
  );
}
