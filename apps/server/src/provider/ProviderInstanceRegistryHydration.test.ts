import {
  DEFAULT_SERVER_SETTINGS,
  ProjectId,
  ProviderDriverKind,
  ProviderInstanceId,
  OnePasswordAccount,
  SecretProviderId,
  type ServerSettings,
} from "@t3tools/contracts";
import { describe, expect, it } from "vite-plus/test";

import {
  changedSecretProviderIds,
  deriveProviderInstanceConfigMap,
} from "./ProviderInstanceRegistryHydration.ts";

const projectId = ProjectId.make("project-a");
const instanceId = ProviderInstanceId.make("codex_work");

function settingsWith(overrides: Partial<ServerSettings>): ServerSettings {
  return { ...DEFAULT_SERVER_SETTINGS, ...overrides };
}

describe("deriveProviderInstanceConfigMap", () => {
  it("turns a machine-disabled instance on when a project enables it", () => {
    const settings = settingsWith({
      providerInstances: {
        [instanceId]: { driver: ProviderDriverKind.make("codex"), enabled: false },
      },
      projectSettingsOverrides: {
        [projectId]: { providerInstances: { [instanceId]: { enabled: true } } },
      },
    });

    const merged = deriveProviderInstanceConfigMap(settings);

    expect(merged[instanceId]).toEqual({ driver: ProviderDriverKind.make("codex"), enabled: true });
  });

  it("clears a nested config.enabled:false flag when a project enables the instance", () => {
    const settings = settingsWith({
      providerInstances: {
        [instanceId]: {
          driver: ProviderDriverKind.make("codex"),
          config: { enabled: false },
        },
      },
      projectSettingsOverrides: {
        [projectId]: { providerInstances: { [instanceId]: { enabled: true } } },
      },
    });

    const merged = deriveProviderInstanceConfigMap(settings);

    expect(merged[instanceId]).toEqual({
      driver: ProviderDriverKind.make("codex"),
      enabled: true,
      config: { enabled: true },
    });
  });

  it("leaves an already machine-enabled instance untouched", () => {
    const settings = settingsWith({
      providerInstances: {
        [instanceId]: { driver: ProviderDriverKind.make("codex"), enabled: true },
      },
      projectSettingsOverrides: {
        [projectId]: { providerInstances: { [instanceId]: { enabled: true } } },
      },
    });

    const merged = deriveProviderInstanceConfigMap(settings);

    expect(merged[instanceId]).toBe(settings.providerInstances[instanceId]);
  });

  it("does not revive an instance a project override references but settings no longer configure", () => {
    const settings = settingsWith({
      providerInstances: {},
      projectSettingsOverrides: {
        [projectId]: { providerInstances: { [instanceId]: { enabled: true } } },
      },
    });

    const merged = deriveProviderInstanceConfigMap(settings);

    expect(merged[instanceId]).toBeUndefined();
  });

  it("ignores a project override that disables an instance, leaving the machine's config in charge", () => {
    const settings = settingsWith({
      providerInstances: {
        [instanceId]: { driver: ProviderDriverKind.make("codex"), enabled: true },
      },
      projectSettingsOverrides: {
        [projectId]: { providerInstances: { [instanceId]: { enabled: false } } },
      },
    });

    const merged = deriveProviderInstanceConfigMap(settings);

    expect(merged[instanceId]).toBe(settings.providerInstances[instanceId]);
  });

  it("folds overrides from multiple projects", () => {
    const otherInstanceId = ProviderInstanceId.make("claudeAgent_personal");
    const otherProjectId = ProjectId.make("project-b");
    const settings = settingsWith({
      providerInstances: {
        [instanceId]: { driver: ProviderDriverKind.make("codex"), enabled: false },
        [otherInstanceId]: { driver: ProviderDriverKind.make("claudeAgent"), enabled: false },
      },
      projectSettingsOverrides: {
        [projectId]: { providerInstances: { [instanceId]: { enabled: true } } },
        [otherProjectId]: { providerInstances: { [otherInstanceId]: { enabled: true } } },
      },
    });

    const merged = deriveProviderInstanceConfigMap(settings);

    expect(merged[instanceId]).toEqual({ driver: ProviderDriverKind.make("codex"), enabled: true });
    expect(merged[otherInstanceId]).toEqual({
      driver: ProviderDriverKind.make("claudeAgent"),
      enabled: true,
    });
  });
});

describe("changedSecretProviderIds", () => {
  const home = SecretProviderId.make("home");
  const vault = SecretProviderId.make("vault");
  const homeConfig = {
    kind: "1password" as const,
    label: "Home",
    account: OnePasswordAccount.make("my.1password.com"),
  };
  const vaultConfig = {
    kind: "openbao" as const,
    label: "Vault",
    address: "https://bao.example.com",
    mount: "secret",
    token: "s.old",
  };

  it("is empty when no provider changed", () => {
    const providers = { [home]: homeConfig, [vault]: vaultConfig };
    expect(changedSecretProviderIds(providers, { ...providers })).toEqual(new Set());
  });

  it("names providers that were added, edited, or removed", () => {
    expect(
      changedSecretProviderIds(
        { [home]: homeConfig },
        { [vault]: { ...vaultConfig, token: "s.new" } },
      ),
    ).toEqual(new Set([home, vault]));
    expect(
      changedSecretProviderIds(
        { [vault]: vaultConfig },
        { [vault]: { ...vaultConfig, token: "s.new" } },
      ),
    ).toEqual(new Set([vault]));
  });
});
