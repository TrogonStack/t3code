import { ProjectId, ProviderInstanceId, type ServerSettings } from "@t3tools/contracts";
import { describe, expect, it } from "vite-plus/test";

import {
  profileAssignmentLabel,
  removeProfilePatch,
  setProfilePatch,
  setProjectProfilePatch,
  toggleProfileInstance,
} from "./providerProfiles.logic";

const projectId = ProjectId.make("work");
const instanceId = ProviderInstanceId.make("company-codex");
const profile = { name: "Company", providerInstanceIds: [instanceId] };

describe("provider profile patches", () => {
  it("upserts a single profile and toggles only its named provider instances", () => {
    expect(setProfilePatch("company", profile)).toEqual({
      trogonStack: { providerProfiles: { company: profile } },
    });
    expect(toggleProfileInstance(profile, instanceId, true)).toEqual(profile);
    expect(toggleProfileInstance(profile, instanceId, false)).toEqual({
      ...profile,
      providerInstanceIds: [],
    });
    expect(
      toggleProfileInstance({ ...profile, providerInstanceIds: [] }, instanceId, true),
    ).toEqual(profile);
  });

  it("assigns or unassigns one project without replacing other assignments", () => {
    expect(setProjectProfilePatch(projectId, "company")).toEqual({
      trogonStack: { projectProfiles: { [projectId]: "company" } },
    });
    expect(setProjectProfilePatch(projectId, null)).toEqual({
      trogonStack: { projectProfiles: { [projectId]: null } },
    });
  });

  it("removes a profile without restoring unrestricted access to assigned projects", () => {
    const settings = {
      trogonStack: {
        providerProfiles: {
          company: profile,
          personal: { name: "Personal", providerInstanceIds: [] },
        },
        projectProfiles: {
          [projectId]: "company",
          [ProjectId.make("other")]: "personal",
          [ProjectId.make("missing")]: "deleted",
        },
        projectsRequiringProfile: { [projectId]: true },
      },
    } satisfies Pick<ServerSettings, "trogonStack">;

    expect(removeProfilePatch("company")).toEqual({
      trogonStack: {
        providerProfiles: { company: null },
      },
    });
    expect(profileAssignmentLabel(settings, projectId)).toBe("Company");
    expect(profileAssignmentLabel(settings, ProjectId.make("missing"))).toBe(
      "Missing profile (providers blocked)",
    );
    expect(profileAssignmentLabel(settings, ProjectId.make("new"))).toBe(
      "No profile (legacy access)",
    );
    const pendingSettings = {
      trogonStack: {
        ...settings.trogonStack,
        projectsRequiringProfile: { [ProjectId.make("new")]: true },
      },
    };
    expect(profileAssignmentLabel(pendingSettings, ProjectId.make("new"))).toBe(
      "Choose a profile before using providers",
    );
  });
});
