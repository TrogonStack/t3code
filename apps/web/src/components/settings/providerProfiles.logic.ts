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

export function profileAssignmentLabel(
  settings: Pick<ServerSettings, "trogonStack">,
  projectId: ProjectId,
): string {
  const profileId = settings.trogonStack.projectProfiles[projectId];
  if (!profileId)
    return projectRequiresProfile(settings, projectId)
      ? "Choose a profile before using providers"
      : "No profile (legacy access)";
  return (
    settings.trogonStack.providerProfiles[profileId]?.name ?? "Missing profile (providers blocked)"
  );
}

export function projectRequiresProfile(
  settings: Pick<ServerSettings, "trogonStack">,
  projectId: string,
): boolean {
  return settings.trogonStack.projectsRequiringProfile[projectId as ProjectId] === true;
}
