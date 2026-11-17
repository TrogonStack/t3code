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
import { ChevronDownIcon, MoreVertical, PlusIcon } from "lucide-react";
import { useState } from "react";

import { useEnvironmentSettings, useUpdateEnvironmentSettings } from "../../hooks/useSettings";
import { randomUUID } from "../../lib/utils";
import { ensureLocalApi } from "../../localApi";
import { useProjects } from "../../state/entities";
import { useEnvironmentScope } from "../../state/session";
import { Button } from "../ui/button";
import {
  Dialog,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogPopup,
  DialogTitle,
} from "../ui/dialog";
import { Input } from "../ui/input";
import { Menu, MenuCheckboxItem, MenuItem, MenuPopup, MenuTrigger } from "../ui/menu";
import { Select, SelectItem, SelectPopup, SelectTrigger, SelectValue } from "../ui/select";
import { useProviderInstanceOptions } from "./providerInstanceRows";
import {
  profileNameTaken,
  profileProviderLabels,
  profileUsageText,
  projectDisplayTitles,
  projectProfileDescription,
  projectsUsingProfile,
  removeProfilePatch,
  setProfilePatch,
  setProjectProfilePatch,
  sortedProfiles,
  toggleProfileInstance,
  type ProviderInstanceOption,
} from "./providerProfiles.logic";
import { SettingsPageContainer, SettingsRow, SettingsSection } from "./settingsLayout";

type SettingsWriter = (
  patch: ServerSettingsPatch,
  options?: { readonly onSaved?: () => void; readonly onFailed?: () => void },
) => void;

const NO_PROFILE = "__no_provider_profile__";

export function ProviderInstanceProfiles({
  settings,
  instanceId,
  disabled,
  onPatch,
}: {
  settings: ServerSettings;
  instanceId: ProviderInstanceId;
  disabled: boolean;
  onPatch: SettingsWriter;
}) {
  const [saving, setSaving] = useState(false);
  const profiles = sortedProfiles(settings);
  if (profiles.length === 0) return null;
  const selected = profiles
    .filter(([, profile]) => profile.providerInstanceIds.includes(instanceId))
    .map(([, profile]) => profile.name);
  const label = selected.length ? selected.join(", ") : "No profiles";
  return (
    <SettingsRow
      title="Profiles"
      description="Projects on a selected profile can use this provider."
      control={
        <Menu>
          <MenuTrigger
            render={<Button variant="outline" size="sm" className="max-w-56" />}
            aria-label={`Profiles: ${label}`}
            disabled={disabled}
          >
            <span className="truncate">{label}</span>
            <ChevronDownIcon />
          </MenuTrigger>
          <MenuPopup align="end">
            {profiles.map(([profileId, profile]) => (
              <MenuCheckboxItem
                key={profileId}
                checked={profile.providerInstanceIds.includes(instanceId)}
                disabled={disabled || saving}
                onCheckedChange={(checked) => {
                  setSaving(true);
                  onPatch(
                    setProfilePatch(profileId, toggleProfileInstance(profile, instanceId, checked)),
                    { onSaved: () => setSaving(false), onFailed: () => setSaving(false) },
                  );
                }}
              >
                {profile.name}
              </MenuCheckboxItem>
            ))}
          </MenuPopup>
        </Menu>
      }
    />
  );
}

function RenameProfileDialog({
  settings,
  profileId,
  profile,
  onSave,
  onClose,
}: {
  settings: ServerSettings;
  profileId: string;
  profile: ProviderProfile;
  onSave: (name: string) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState(profile.name);
  const trimmed = name.trim();
  const taken = profileNameTaken(settings, trimmed, profileId);
  const valid = trimmed !== "" && trimmed !== profile.name && !taken;
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogPopup>
        <form
          className="flex min-h-0 flex-col"
          onSubmit={(event) => {
            event.preventDefault();
            if (valid) onSave(trimmed);
          }}
        >
          <DialogHeader>
            <DialogTitle>Rename profile</DialogTitle>
          </DialogHeader>
          <DialogPanel>
            <div className="flex flex-col gap-2">
              <Input
                autoFocus
                aria-label="Profile name"
                value={name}
                onChange={(event) => setName(event.currentTarget.value)}
              />
              {taken ? (
                <p className="text-xs text-destructive-foreground">
                  Another profile already has that name.
                </p>
              ) : null}
            </div>
          </DialogPanel>
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={!valid}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogPopup>
    </Dialog>
  );
}

function ProfileRow({
  settings,
  profileId,
  profile,
  usedBy,
  options,
  disabled,
  onPatch,
}: {
  settings: ServerSettings;
  profileId: string;
  profile: ProviderProfile;
  usedBy: ReadonlyArray<string>;
  options: ReadonlyArray<ProviderInstanceOption>;
  disabled: boolean;
  onPatch: SettingsWriter;
}) {
  const [saving, setSaving] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const save = (patch: ServerSettingsPatch, onSaved?: () => void) => {
    setSaving(true);
    onPatch(patch, {
      onSaved: () => {
        setSaving(false);
        onSaved?.();
      },
      onFailed: () => setSaving(false),
    });
  };
  const labels = profileProviderLabels(profile, options);
  const buttonLabel = labels.length ? labels.join(", ") : "No providers";
  const inUse = usedBy.length > 0;
  const known = new Set(options.map(({ instanceId }) => instanceId));
  const removedIds = profile.providerInstanceIds.filter((instanceId) => !known.has(instanceId));
  const remove = async () => {
    const confirmed = await ensureLocalApi().dialogs.confirm(
      `Delete the ${profile.name} provider profile?`,
      { variant: "destructive" },
    );
    if (confirmed) save(removeProfilePatch(profileId));
  };
  const setMember = (instanceId: ProviderInstanceId, checked: boolean) =>
    save(setProfilePatch(profileId, toggleProfileInstance(profile, instanceId, checked)));

  return (
    <SettingsRow
      title={profile.name}
      description={`${profileUsageText(usedBy)}${inUse ? " Move them to another profile to delete this one." : ""}`}
      control={
        <div className="flex items-center gap-2">
          <Menu>
            <MenuTrigger
              render={<Button variant="outline" size="sm" className="max-w-56" />}
              aria-label={`Providers for ${profile.name}: ${buttonLabel}`}
              disabled={disabled || saving}
            >
              <span className="truncate">{buttonLabel}</span>
              <ChevronDownIcon />
            </MenuTrigger>
            <MenuPopup align="end">
              {options.map(({ instanceId, label }) => (
                <MenuCheckboxItem
                  key={instanceId}
                  checked={profile.providerInstanceIds.includes(instanceId)}
                  onCheckedChange={(checked) => setMember(instanceId, checked)}
                >
                  {label}
                </MenuCheckboxItem>
              ))}
              {removedIds.map((instanceId) => (
                <MenuCheckboxItem
                  key={instanceId}
                  checked
                  onCheckedChange={(checked) => setMember(instanceId, checked)}
                >
                  {instanceId} (removed)
                </MenuCheckboxItem>
              ))}
            </MenuPopup>
          </Menu>
          <Menu>
            <MenuTrigger
              render={
                <Button
                  size="icon-sm"
                  variant="ghost-muted"
                  aria-label={`More actions for ${profile.name}`}
                />
              }
              disabled={disabled || saving}
            >
              <MoreVertical />
            </MenuTrigger>
            <MenuPopup align="end">
              <MenuItem onClick={() => setRenaming(true)}>Rename</MenuItem>
              <MenuItem variant="destructive" disabled={inUse} onClick={() => void remove()}>
                Delete
              </MenuItem>
            </MenuPopup>
          </Menu>
          {renaming ? (
            <RenameProfileDialog
              settings={settings}
              profileId={profileId}
              profile={profile}
              onClose={() => setRenaming(false)}
              onSave={(name) =>
                save(setProfilePatch(profileId, { ...profile, name }), () => setRenaming(false))
              }
            />
          ) : null}
        </div>
      }
    />
  );
}

function NewProfileRow({
  settings,
  disabled,
  onPatch,
}: {
  settings: ServerSettings;
  disabled: boolean;
  onPatch: SettingsWriter;
}) {
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  const name = newName.trim();
  const taken = profileNameTaken(settings, name);
  const create = () => {
    if (!name || taken || creating) return;
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
    <SettingsRow
      title="New profile"
      description={
        taken
          ? "Another profile already has that name."
          : "Name it after the boundary it keeps, like Personal or a company."
      }
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
            disabled={disabled || creating || !name || taken}
            onClick={create}
          >
            <PlusIcon /> Create
          </Button>
        </div>
      }
    />
  );
}

function ProjectProfileRow({
  settings,
  projectId,
  title,
  ariaLabel,
  options,
  disabled,
  onPatch,
}: {
  settings: ServerSettings;
  projectId: ProjectId;
  title: string;
  ariaLabel: string;
  options: ReadonlyArray<ProviderInstanceOption>;
  disabled: boolean;
  onPatch: SettingsWriter;
}) {
  const [saving, setSaving] = useState(false);
  const profileId = settings.trogonStack.projectProfiles[projectId];
  const profile =
    profileId === undefined ? undefined : settings.trogonStack.providerProfiles[profileId];
  return (
    <SettingsRow
      title={title}
      description={projectProfileDescription(settings, projectId, options)}
      control={
        <div className="w-full sm:w-60">
          <Select
            value={profile && profileId ? profileId : NO_PROFILE}
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
            <SelectTrigger size="sm" aria-label={ariaLabel}>
              <SelectValue>{profile?.name ?? "No profile"}</SelectValue>
            </SelectTrigger>
            <SelectPopup>
              <SelectItem value={NO_PROFILE}>No profile</SelectItem>
              {sortedProfiles(settings).map(([id, candidate]) => (
                <SelectItem key={id} value={id}>
                  {candidate.name}
                </SelectItem>
              ))}
            </SelectPopup>
          </Select>
        </div>
      }
    />
  );
}

function useProfileWriteAccess(environmentId: EnvironmentId) {
  const canWriteSettings = useEnvironmentScope(environmentId, AuthSettingsWriteScope);
  const canManageProviders = useEnvironmentScope(environmentId, AuthProvidersManageScope);
  return canWriteSettings && canManageProviders;
}

export function ProjectProviderProfileAssignment({
  environmentId,
  projectId,
  title = "Provider profile",
  available = true,
}: {
  environmentId: EnvironmentId;
  projectId: ProjectId;
  title?: string;
  available?: boolean;
}) {
  const settings = useEnvironmentSettings(environmentId);
  const options = useProviderInstanceOptions(environmentId);
  const canWrite = useProfileWriteAccess(environmentId);
  const updateSettings = useUpdateEnvironmentSettings(environmentId);
  return (
    <ProjectProfileRow
      settings={settings}
      projectId={projectId}
      title={title}
      ariaLabel={title}
      options={options}
      disabled={!available || !canWrite}
      onPatch={updateSettings}
    />
  );
}

export function ProviderProfilesView({
  settings,
  options,
  projects,
  disabled,
  onPatch,
}: {
  settings: ServerSettings;
  options: ReadonlyArray<ProviderInstanceOption>;
  projects: ReadonlyArray<{ id: ProjectId; title: string; workspaceRoot: string }>;
  disabled: boolean;
  onPatch: SettingsWriter;
}) {
  const titles = projectDisplayTitles(projects);
  const projectRows = projects
    .map((project) => ({ id: project.id, title: titles.get(project.id) ?? project.title }))
    .toSorted((left, right) => left.title.localeCompare(right.title));

  return (
    <SettingsPageContainer>
      <p className="text-sm text-muted-foreground">
        A profile is a group of providers. A project on a profile can only use the providers in that
        group.
      </p>
      {disabled ? (
        <p className="text-xs text-muted-foreground">
          You do not have permission to change provider profiles on this environment.
        </p>
      ) : null}
      <SettingsSection id="provider-profiles-list" title="Profiles">
        <NewProfileRow settings={settings} disabled={disabled} onPatch={onPatch} />
        {sortedProfiles(settings).map(([profileId, profile]) => (
          <ProfileRow
            key={profileId}
            settings={settings}
            profileId={profileId}
            profile={profile}
            usedBy={projectsUsingProfile(settings, profileId, projects).map(
              (project) => titles.get(project.id) ?? project.title,
            )}
            options={options}
            disabled={disabled}
            onPatch={onPatch}
          />
        ))}
      </SettingsSection>
      <SettingsSection id="provider-profiles-projects" title="Projects">
        {projectRows.length === 0 ? (
          <p className="px-4 py-3 text-sm text-muted-foreground">No projects yet.</p>
        ) : (
          projectRows.map((project) => (
            <ProjectProfileRow
              key={project.id}
              settings={settings}
              projectId={project.id}
              title={project.title}
              ariaLabel={`Provider profile for ${project.title}`}
              options={options}
              disabled={disabled}
              onPatch={onPatch}
            />
          ))
        )}
      </SettingsSection>
    </SettingsPageContainer>
  );
}

export function ProviderProfilesPage({ environmentId }: { environmentId: EnvironmentId }) {
  const settings = useEnvironmentSettings(environmentId);
  const options = useProviderInstanceOptions(environmentId);
  const canWrite = useProfileWriteAccess(environmentId);
  const updateSettings = useUpdateEnvironmentSettings(environmentId);
  const projects = useProjects().filter((project) => project.environmentId === environmentId);
  return (
    <ProviderProfilesView
      settings={settings}
      options={options}
      projects={projects}
      disabled={!canWrite}
      onPatch={updateSettings}
    />
  );
}
