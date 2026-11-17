import type {
  ProjectId,
  ProviderInstanceId,
  ProviderProfile,
  ServerSettings,
  ServerSettingsPatch,
} from "@t3tools/contracts";

export function setProfilePatch(profileId: string, profile: ProviderProfile): ServerSettingsPatch {
  return { trogonStack: { providerProfiles: { [profileId]: profile } } };
}

export function setProjectProfilePatch(
  projectId: ProjectId,
  profileId: string | null,
): ServerSettingsPatch {
  return { trogonStack: { projectProfiles: { [projectId]: profileId } } };
}

export function toggleProfileInstance(
  profile: ProviderProfile,
  instanceId: ProviderInstanceId,
  allowed: boolean,
): ProviderProfile {
  const included = profile.providerInstanceIds.includes(instanceId);
  if (included === allowed) return profile;
  return {
    ...profile,
    providerInstanceIds: allowed
      ? [...profile.providerInstanceIds, instanceId]
      : profile.providerInstanceIds.filter((candidate) => candidate !== instanceId),
  };
}

export function removeProfilePatch(profileId: string): ServerSettingsPatch {
  return { trogonStack: { providerProfiles: { [profileId]: null } } };
}

export interface ProviderInstanceOption {
  readonly instanceId: ProviderInstanceId;
  readonly label: string;
}

type ProfileSettings = Pick<ServerSettings, "trogonStack">;

export function sortedProfiles(settings: ProfileSettings) {
  return Object.entries(settings.trogonStack.providerProfiles).toSorted((left, right) =>
    left[1].name.localeCompare(right[1].name),
  );
}

export function profileNameTaken(
  settings: ProfileSettings,
  name: string,
  exceptProfileId?: string,
): boolean {
  const wanted = name.trim().toLowerCase();
  return Object.entries(settings.trogonStack.providerProfiles).some(
    ([profileId, profile]) =>
      profileId !== exceptProfileId && profile.name.trim().toLowerCase() === wanted,
  );
}

export function profileProviderLabels(
  profile: ProviderProfile,
  options: ReadonlyArray<ProviderInstanceOption>,
): string[] {
  const known = new Set(options.map(({ instanceId }) => instanceId));
  return [
    ...options
      .filter(({ instanceId }) => profile.providerInstanceIds.includes(instanceId))
      .map(({ label }) => label),
    ...profile.providerInstanceIds
      .filter((instanceId) => !known.has(instanceId))
      .map((instanceId) => `${instanceId} (removed)`),
  ];
}

export function profileUsageText(
  settings: ProfileSettings,
  profileId: string,
  titles: ReadonlyMap<ProjectId, string>,
): string {
  const assigned = Object.entries(settings.trogonStack.projectProfiles)
    .filter(([, assignedId]) => assignedId === profileId)
    .map(([projectId]) => titles.get(projectId as ProjectId));
  if (assigned.length === 0) return "No project uses it yet.";
  const known = assigned
    .filter((title) => title !== undefined)
    .toSorted((left, right) => left.localeCompare(right));
  const hidden = assigned.length - known.length;
  const names =
    hidden > 0 ? [...known, `${hidden} other project${hidden === 1 ? "" : "s"}`] : known;
  return `Used by ${names.join(", ")}. Move them to another profile to delete this one.`;
}

export function profileInUse(settings: ProfileSettings, profileId: string): boolean {
  return Object.values(settings.trogonStack.projectProfiles).includes(profileId);
}

/** Titles as the user knows the projects; checkouts sharing a title also show their folder. */
export function projectDisplayTitles(
  projects: ReadonlyArray<{
    readonly id: ProjectId;
    readonly title: string;
    readonly workspaceRoot: string;
  }>,
): Map<ProjectId, string> {
  const counts = new Map<string, number>();
  for (const { title } of projects) counts.set(title, (counts.get(title) ?? 0) + 1);
  return new Map(
    projects.map(({ id, title, workspaceRoot }) => [
      id,
      (counts.get(title) ?? 0) > 1 ? `${title} (${workspaceRoot})` : title,
    ]),
  );
}

export function projectProfileDescription(
  settings: ProfileSettings,
  projectId: ProjectId,
  options: ReadonlyArray<ProviderInstanceOption>,
): string {
  const profileId = settings.trogonStack.projectProfiles[projectId];
  if (profileId === undefined) {
    return projectRequiresProfile(settings, projectId)
      ? "Blocked until you pick a profile."
      : "Not restricted. Can use every provider.";
  }
  const profile = settings.trogonStack.providerProfiles[profileId];
  if (!profile) return "Its profile was deleted. Blocked until you pick another.";
  const labels = profileProviderLabels(profile, options);
  return labels.length
    ? `Can use ${labels.join(", ")}.`
    : `Can use no providers. Add some to ${profile.name}.`;
}

function projectRequiresProfile(settings: ProfileSettings, projectId: ProjectId): boolean {
  return settings.trogonStack.projectsRequiringProfile[projectId] === true;
}
