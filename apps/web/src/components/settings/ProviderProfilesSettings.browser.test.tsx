import {
  DEFAULT_SERVER_SETTINGS,
  ProjectId,
  ProviderDriverKind,
  ProviderInstanceId,
  type ServerSettings,
  type ServerSettingsPatch,
} from "@t3tools/contracts";
import { expect, it, vi } from "vite-plus/test";

import "~/index.css";
import { render } from "../../../test/render";
import { ProviderInstanceProfiles, ProviderProfilesView } from "./ProviderProfilesSettings";

const instanceId = ProviderInstanceId.make("company-codex");
const projectId = ProjectId.make("client");
const options = [{ instanceId, label: "Company Codex" }];
const projects = [{ id: projectId, title: "client", workspaceRoot: "/work/client" }];
const settings: ServerSettings = {
  ...DEFAULT_SERVER_SETTINGS,
  providerInstances: {
    [instanceId]: { driver: ProviderDriverKind.make("codex"), displayName: "Company Codex" },
  },
  trogonStack: {
    providerProfiles: { company: { name: "Company", providerInstanceIds: [] } },
    projectProfiles: { [projectId]: "company" },
    projectsRequiringProfile: {},
  },
};

it("sends targeted patches when renaming a profile", async () => {
  const onPatch = vi.fn((_: ServerSettingsPatch, options?: { onSaved?: () => void }) =>
    options?.onSaved?.(),
  );
  await using screen = await render(
    <ProviderProfilesView
      settings={settings}
      options={options}
      projects={projects}
      disabled={false}
      onPatch={onPatch}
    />,
  );

  expect(screen.getByText(/Used by client\./)).toBeVisible();
  await screen.getByRole("button", { name: "More actions for Company" }).click();
  await screen.getByRole("menuitem", { name: "Rename" }).click();
  await screen.getByRole("textbox", { name: "Profile name" }).fill("New name");
  await screen.getByRole("button", { name: "Save" }).click();
  expect(onPatch).toHaveBeenLastCalledWith(
    {
      trogonStack: { providerProfiles: { company: { name: "New name", providerInstanceIds: [] } } },
    },
    expect.any(Object),
  );
});

it("adds an instance to a profile from the instance's own settings", async () => {
  const onPatch = vi.fn((_: ServerSettingsPatch, options?: { onSaved?: () => void }) =>
    options?.onSaved?.(),
  );
  await using screen = await render(
    <ProviderInstanceProfiles
      settings={settings}
      instanceId={instanceId}
      disabled={false}
      onPatch={onPatch}
    />,
  );

  await screen.getByRole("button", { name: "Profiles: No profiles" }).click();
  await screen.getByRole("menuitemcheckbox", { name: "Company" }).click();
  expect(onPatch).toHaveBeenLastCalledWith(
    {
      trogonStack: {
        providerProfiles: { company: { name: "Company", providerInstanceIds: [instanceId] } },
      },
    },
    expect.any(Object),
  );
});

it("starts new profiles with no allowed providers", async () => {
  const onPatch = vi.fn((_: ServerSettingsPatch, options?: { onSaved?: () => void }) =>
    options?.onSaved?.(),
  );
  await using screen = await render(
    <ProviderProfilesView
      settings={settings}
      options={options}
      projects={projects}
      disabled={false}
      onPatch={onPatch}
    />,
  );

  await screen.getByRole("textbox", { name: "New provider profile name" }).fill("Personal");
  await screen.getByRole("button", { name: "Create" }).click();
  const [patch] = onPatch.mock.lastCall!;
  expect(Object.values(patch.trogonStack?.providerProfiles ?? {})).toEqual([
    { name: "Personal", providerInstanceIds: [] },
  ]);
  expect(Object.keys(patch.trogonStack?.providerProfiles ?? {})).toHaveLength(1);
});

it("assigns a project to a profile or removes its profile", async () => {
  const onPatch = vi.fn((_: ServerSettingsPatch, options?: { onSaved?: () => void }) =>
    options?.onSaved?.(),
  );
  await using screen = await render(
    <ProviderProfilesView
      settings={settings}
      options={options}
      projects={projects}
      disabled={false}
      onPatch={onPatch}
    />,
  );

  expect(screen.getByText("Can use no providers. Add some to Company.")).toBeVisible();
  await screen.getByRole("combobox", { name: "Provider profile for client" }).click();
  await screen.getByRole("option", { name: "No profile" }).click();
  expect(onPatch).toHaveBeenLastCalledWith(
    { trogonStack: { projectProfiles: { [projectId]: null } } },
    expect.any(Object),
  );
});
