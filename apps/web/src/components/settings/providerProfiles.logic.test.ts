import { ProjectId, ProviderInstanceId, type ServerSettings } from "@t3tools/contracts";
import { describe, expect, it } from "vite-plus/test";

import {
  profileNameTaken,
  profileInUse,
  profileUsageText,
  projectDisplayTitles,
  projectProfileDescription,
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
    const options = [{ instanceId, label: "Company Codex" }];
    const description = (value: Pick<ServerSettings, "trogonStack">, id: string) =>
      projectProfileDescription(value, ProjectId.make(id), options);
    expect(description(settings, projectId)).toBe("Can use Company Codex.");
    expect(description(settings, "other")).toBe("Can use no providers. Add some to Personal.");
    expect(description(settings, "missing")).toBe(
      "Its profile was deleted. Blocked until you pick another.",
    );
    expect(description(settings, "new")).toBe("Not restricted. Can use every provider.");
    expect(
      description(
        {
          trogonStack: {
            ...settings.trogonStack,
            projectsRequiringProfile: { [ProjectId.make("new")]: true },
          },
        },
        "new",
      ),
    ).toBe("Blocked until you pick a profile.");
    expect(projectProfileDescription(settings, projectId, [])).toBe(
      "Can use company-codex (removed).",
    );
  });

  it("words who uses a profile, tells apart same-named projects, and rejects duplicate names", () => {
    const usage = {
      trogonStack: {
        providerProfiles: { company: profile },
        projectProfiles: {
          [ProjectId.make("w")]: "company",
          [ProjectId.make("t")]: "company",
          [ProjectId.make("gone")]: "company",
        },
        projectsRequiringProfile: {},
      },
    };
    const known = new Map([
      [ProjectId.make("w"), "website"],
      [ProjectId.make("t"), "t3code"],
    ]);
    expect(profileUsageText(usage, "personal", known)).toBe("No project uses it yet.");
    expect(profileUsageText(usage, "company", known)).toBe(
      "Used by t3code, website, 1 other project. Move them to another profile to delete this one.",
    );
    expect(profileInUse(usage, "company")).toBe(true);
    expect(profileInUse(usage, "personal")).toBe(false);
    const titles = projectDisplayTitles([
      { id: ProjectId.make("a"), title: "app", workspaceRoot: "/one/app" },
      { id: ProjectId.make("b"), title: "app", workspaceRoot: "/two/app" },
      { id: ProjectId.make("c"), title: "site", workspaceRoot: "/site" },
    ]);
    expect([...titles.values()]).toEqual(["app (/one/app)", "app (/two/app)", "site"]);
    const settings = {
      trogonStack: {
        providerProfiles: { company: profile },
        projectProfiles: {},
        projectsRequiringProfile: {},
      },
    };
    expect(profileNameTaken(settings, " company ")).toBe(true);
    expect(profileNameTaken(settings, "Company", "company")).toBe(false);
    expect(profileNameTaken(settings, "Personal")).toBe(false);
  });
});
