import {
  DEFAULT_SERVER_SETTINGS,
  ProjectId,
  ProviderDriverKind,
  ProviderInstanceId,
  type ServerSettings,
  type ServerSettingsPatch,
} from "@t3tools/contracts";
import { expect, it, vi } from "vite-plus/test";
import { userEvent } from "vite-plus/test/browser";

import "~/index.css";
import { render } from "../../../test/render";
import { ProviderProfilesSettings } from "./ProviderProfilesSettings";

const instanceId = ProviderInstanceId.make("company-codex");
const projectId = ProjectId.make("client");
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

it("sends targeted patches when editing a profile's name and allowed instances", async () => {
  const onPatch = vi.fn((_: ServerSettingsPatch, options?: { onSaved?: () => void }) =>
    options?.onSaved?.(),
  );
  await using screen = await render(
    <ProviderProfilesSettings
      settings={settings}
      providers={[]}
      disabled={false}
      onPatch={onPatch}
    />,
  );

  await screen.getByRole("checkbox", { name: "Company: Company Codex" }).click();
  expect(onPatch).toHaveBeenLastCalledWith(
    {
      trogonStack: {
        providerProfiles: { company: { name: "Company", providerInstanceIds: [instanceId] } },
      },
    },
    expect.any(Object),
  );

  const name = screen.getByRole("textbox", { name: "Rename provider profile Company" });
  await name.fill("New name");
  await userEvent.tab();
  expect(onPatch).toHaveBeenLastCalledWith(
    {
      trogonStack: { providerProfiles: { company: { name: "New name", providerInstanceIds: [] } } },
    },
    expect.any(Object),
  );
});

it("starts new profiles with no allowed providers", async () => {
  const onPatch = vi.fn((_: ServerSettingsPatch, options?: { onSaved?: () => void }) =>
    options?.onSaved?.(),
  );
  await using screen = await render(
    <ProviderProfilesSettings
      settings={settings}
      providers={[]}
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
