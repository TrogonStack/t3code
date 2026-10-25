import { RegistryContext } from "@effect/atom-react";
import {
  DEFAULT_UNIFIED_SETTINGS,
  EnvironmentId,
  ProviderDriverKind,
  OpenBaoSecretProviderConfig,
  ProviderInstanceEnvironment,
  ProviderInstanceId,
  SecretProviderId,
  type ServerProvider,
  type UnifiedSettings,
} from "@t3tools/contracts";
import * as Schema from "effect/Schema";
import { AsyncResult, type Atom } from "effect/reactivity";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vite-plus/test";
import { page, userEvent } from "vite-plus/test/browser";

import "~/index.css";
import { act, render } from "../../../test/render";

const state = vi.hoisted(() => ({
  providers: null as unknown as Atom.Writable<ReadonlyArray<ServerProvider>>,
  settings: null as unknown as UnifiedSettings,
  canManageProviders: true,
  canWriteSettings: true,
  searchTargetId: null as string | null,
  readEnvironmentIds: [] as string[],
  updateEnvironmentIds: [] as string[],
  mutationEnvironmentIds: [] as string[],
  refresh: vi.fn(),
  updateProvider: vi.fn(),
  uninstall: vi.fn(),
  acceptUrlAuth: vi.fn(),
  updateSettings: vi.fn(),
  updateModelPreferences: vi.fn(),
  toggleFavorite: vi.fn(),
  mutateProviderInstance: vi.fn(),
  checkSecretSource: vi.fn(),
}));

vi.mock(import("../../state/server"), async (original) => {
  const actual = await original();
  const { Atom } = await import("effect/reactivity");
  const providers = Atom.make<ReadonlyArray<ServerProvider>>([]).pipe(Atom.keepAlive);
  state.providers = providers;
  const config = Atom.make(null);
  const commandLabels = {
    refreshProviders: "refresh",
    updateProvider: "update",
    uninstallAcpRegistryManagedBinary: "uninstall",
    acceptAcpRegistryUrlAuth: "accept-url-auth",
  } as const;
  const labelled = Object.fromEntries(
    Object.entries(commandLabels).map(([key, label]) => [
      key,
      { ...actual.serverEnvironment[key as keyof typeof commandLabels], label },
    ]),
  );
  return {
    ...actual,
    providersIgnoringCheckedAtAtom: (() => providers) as never,
    serverEnvironment: {
      ...actual.serverEnvironment,
      ...labelled,
      checkSecretSource: {
        ...actual.serverEnvironment.checkSecretSource,
        label: "check-secret-source",
        permissionAtom: (() => Atom.make(true)) as never,
      },
      configValueAtom: (() => config) as never,
      providersValueAtom: (() => providers) as never,
    } as never,
  };
});
vi.mock(import("../../state/use-atom-command"), async (original) => ({
  ...(await original()),
  useAtomCommand: ((command: { label?: string }) =>
    ({
      refresh: state.refresh,
      update: state.updateProvider,
      uninstall: state.uninstall,
      "accept-url-auth": state.acceptUrlAuth,
      "check-secret-source": state.checkSecretSource,
    })[command.label ?? ""] ?? vi.fn()) as never,
}));
vi.mock(import("../../state/query"), async (original) => ({
  ...(await original()),
  useEnvironmentQuery: (() => ({
    data: null,
    error: null,
    isPending: false,
    refresh: vi.fn(),
  })) as never,
}));
vi.mock(import("../../hooks/useSettings"), async (original) => ({
  ...(await original()),
  useEnvironmentSettings: ((environmentId: string) => {
    state.readEnvironmentIds.push(environmentId);
    return state.settings;
  }) as never,
  useUpdateEnvironmentSettings: ((environmentId: string) => {
    state.updateEnvironmentIds.push(environmentId);
    return state.updateSettings;
  }) as never,
  useUpdateEnvironmentModelPreferences: (() => state.updateModelPreferences) as never,
  useToggleEnvironmentModelFavorite: (() => state.toggleFavorite) as never,
  usePersistEnvironmentProviderInstanceMutation: ((environmentId: string) => {
    state.mutationEnvironmentIds.push(environmentId);
    return state.mutateProviderInstance;
  }) as never,
}));
vi.mock(import("../../state/session"), async (original) => {
  const scope = (environmentId: string, requested: string) =>
    environmentId === "remote-device" &&
    (requested === "providers:manage"
      ? state.canManageProviders
      : requested === "settings:write"
        ? state.canWriteSettings
        : requested === "orchestration:read");
  return {
    ...(await original()),
    useEnvironmentScope: scope as never,
    readEnvironmentScope: scope as never,
  };
});
vi.mock(import("../../state/entities"), async (original) => ({
  ...(await original()),
  useProjects: (() => []) as never,
}));
vi.mock(import("./settingsLayout"), async (original) => ({
  ...(await original()),
  useSettingsSearchTargetId: () => state.searchTargetId,
  SettingsRow: (({
    id,
    title,
    description,
    status,
    control,
    children,
  }: {
    id?: string;
    title: ReactNode;
    description?: ReactNode;
    status?: ReactNode;
    control?: ReactNode;
    children?: ReactNode;
  }) => (
    <div id={id}>
      <h4>{title}</h4>
      {description}
      {status}
      {control}
      {children}
    </div>
  )) as never,
}));
vi.mock(import("./UsageProviderSettings"), async (original) => ({
  ...(await original()),
  UsageProviderSettings: (() => null) as never,
}));
vi.mock(import("../ProviderUpdatesAction"), async (original) => ({
  ...(await original()),
  ProviderUpdatesAction: (() => null) as never,
}));

import { appAtomRegistry } from "~/rpc/atomRegistry";
import { EnvironmentProviderSettings } from "./ProviderSettingsPanel";

const decodeOpenBaoProvider = Schema.decodeSync(OpenBaoSecretProviderConfig);
const decodeEnvironment = Schema.decodeSync(ProviderInstanceEnvironment);

const environmentId = EnvironmentId.make("remote-device");
const codexId = ProviderInstanceId.make("codex");
const customId = ProviderInstanceId.make("codex_work");

function provider(): ServerProvider {
  return {
    instanceId: codexId,
    driver: ProviderDriverKind.make("codex"),
    enabled: true,
    installed: true,
    version: "1.0.0",
    status: "ready",
    auth: { status: "authenticated" },
    checkedAt: "2026-07-24T12:00:00.000Z",
    models: [{ slug: "chosen", name: "Chosen", isCustom: false, capabilities: null }],
    slashCommands: [],
    skills: [],
    versionAdvisory: {
      status: "behind_latest",
      currentVersion: "1.0.0",
      latestVersion: "1.1.0",
      updateCommand: "pnpm add -g @openai/codex@latest",
      canUpdate: true,
      checkedAt: "2026-07-24T12:00:00.000Z",
      message: "Update available.",
    },
  };
}

const setProviders = (providers: ReadonlyArray<ServerProvider>) =>
  act(async () => appAtomRegistry.set(state.providers, providers));

function panel(options?: {
  readonly readOnly?: boolean;
  readonly targetInstanceId?: ProviderInstanceId;
}) {
  return (
    <RegistryContext.Provider value={appAtomRegistry}>
      <div className="@container/providers">
        <EnvironmentProviderSettings
          environmentId={environmentId}
          environmentLabel="Remote device"
          {...(options?.readOnly === undefined ? {} : { readOnly: options.readOnly })}
          {...(options?.targetInstanceId === undefined
            ? {}
            : { targetInstanceId: options.targetInstanceId })}
        />
      </div>
    </RegistryContext.Provider>
  );
}

// The section header and the end of the provider list both offer Add provider.

async function setup() {
  await page.viewport(1280, 900);
  state.settings = DEFAULT_UNIFIED_SETTINGS;
  state.canManageProviders = true;
  state.canWriteSettings = true;
  state.searchTargetId = null;
  state.readEnvironmentIds = [];
  state.updateEnvironmentIds = [];
  state.mutationEnvironmentIds = [];
  state.updateSettings.mockReset();
  state.updateModelPreferences.mockReset();
  state.toggleFavorite.mockReset();
  state.mutateProviderInstance.mockReset().mockResolvedValue(AsyncResult.success({}));
  state.refresh.mockReset().mockResolvedValue(AsyncResult.success(undefined));
  state.updateProvider.mockReset().mockResolvedValue(AsyncResult.success(undefined));
  state.uninstall.mockReset().mockResolvedValue(AsyncResult.success({}));
  state.acceptUrlAuth.mockReset().mockResolvedValue(AsyncResult.success({ accepted: true }));
  state.checkSecretSource.mockReset();
  await setProviders([]);
}

describe("EnvironmentProviderSettings routing", () => {
  it("does not substitute another account when the requested instance was removed", async () => {
    await setup();
    await setProviders([provider()]);
    await using _screen = await render(panel({ targetInstanceId: customId }));
    await expect
      .element(page.getByText("This provider instance is no longer available on this device."))
      .toBeVisible();
    expect(state.updateSettings).not.toHaveBeenCalled();
  });

  it("updates one provider instance without sending a stale whole map", async () => {
    await setup();
    state.settings = {
      ...DEFAULT_UNIFIED_SETTINGS,
      providerInstances: {
        [customId]: {
          driver: ProviderDriverKind.make("codex"),
          enabled: true,
          displayName: "Work",
        },
      },
    };
    await using _screen = await render(panel());
    await userEvent.click(page.getByRole("switch", { name: "Enable Work" }).first());

    expect(state.mutateProviderInstance).toHaveBeenCalledWith(
      {
        operation: "upsert",
        instanceId: customId,
        instance: { driver: ProviderDriverKind.make("codex"), enabled: false, displayName: "Work" },
      },
      {},
    );
  });

  it("checks a secret reference on the selected environment and shows the outcome", async () => {
    await setup();
    const vault = SecretProviderId.make("vault");
    state.settings = {
      ...DEFAULT_UNIFIED_SETTINGS,
      secretProviders: {
        [vault]: decodeOpenBaoProvider({
          kind: "openbao",
          label: "Team vault",
          address: "https://bao.example.com",
          mount: "kv",
        }),
      },
      providerInstances: {
        [customId]: {
          driver: ProviderDriverKind.make("codex"),
          enabled: true,
          displayName: "Work",
          environment: decodeEnvironment([
            {
              name: "CODEX_TOKEN",
              value: { kind: "secret-provider", provider: vault, reference: "agents/codex#key" },
            },
          ]),
        },
      },
    };
    state.checkSecretSource
      .mockResolvedValueOnce(AsyncResult.success({ _tag: "Failed", detail: "permission denied" }))
      .mockResolvedValueOnce(AsyncResult.success({ _tag: "Resolved" }));
    await using _screen = await render(panel({ targetInstanceId: customId }));

    await userEvent.click(
      page.getByRole("button", { name: "Check secret for environment variable CODEX_TOKEN" }),
    );
    expect(state.checkSecretSource).toHaveBeenLastCalledWith({
      environmentId,
      input: {
        source: { kind: "secret-provider", provider: vault, reference: "agents/codex#key" },
      },
    });
    await expect
      .element(
        page.getByRole("button", {
          name: "Check secret for environment variable CODEX_TOKEN failed: permission denied",
        }),
      )
      .toBeVisible();

    await userEvent.click(
      page.getByRole("button", { name: /^Check secret for environment variable CODEX_TOKEN/ }),
    );
    await expect
      .element(
        page.getByRole("button", {
          name: "Check secret for environment variable CODEX_TOKEN resolved",
        }),
      )
      .toBeVisible();
  });

  it("saves a dedicated field from a named secret provider", async () => {
    await setup();
    const vault = SecretProviderId.make("vault");
    const cursorId = ProviderInstanceId.make("cursor_work");
    const cursor = ProviderDriverKind.make("cursor");
    state.settings = {
      ...DEFAULT_UNIFIED_SETTINGS,
      secretProviders: {
        [vault]: decodeOpenBaoProvider({
          kind: "openbao",
          label: "Team vault",
          address: "https://bao.example.com",
          mount: "kv",
        }),
      },
      providerInstances: {
        [cursorId]: { driver: cursor, enabled: true, displayName: "Cursor Work" },
      },
    };
    await using _screen = await render(panel({ targetInstanceId: cursorId }));

    await userEvent.click(page.getByRole("combobox", { name: "Cursor API key source" }));
    await userEvent.click(page.getByRole("option", { name: /Team vault/ }));
    expect(state.mutateProviderInstance).not.toHaveBeenCalled();

    const reference = page.getByRole("textbox", { name: "Cursor API key secret reference" });
    await userEvent.fill(reference, "agents/cursor#key");
    await userEvent.tab();

    expect(state.mutateProviderInstance).toHaveBeenLastCalledWith(
      {
        operation: "upsert",
        instanceId: cursorId,
        instance: {
          driver: cursor,
          enabled: true,
          displayName: "Cursor Work",
          environment: [
            {
              name: "CURSOR_API_KEY",
              value: { kind: "secret-provider", provider: vault, reference: "agents/cursor#key" },
              sensitive: false,
            },
          ],
        },
      },
      {},
    );
  });

  it("routes explicit ACP browser authentication consent to the selected environment", async () => {
    await setup();
    const instanceId = ProviderInstanceId.make("acpRegistry_antigravity");
    const action = {
      elicitationId: "google-login-1",
      url: "https://accounts.google.com/login",
      message: "Continue with Google",
    };
    state.settings = {
      ...DEFAULT_UNIFIED_SETTINGS,
      providerInstances: {
        [instanceId]: {
          driver: ProviderDriverKind.make("acpRegistry"),
          enabled: true,
          config: { agentId: "antigravity" },
        },
      },
    };
    await setProviders([
      {
        ...provider(),
        instanceId,
        driver: ProviderDriverKind.make("acpRegistry"),
        auth: { status: "unauthenticated", action },
      },
    ]);
    await using _screen = await render(panel({ targetInstanceId: instanceId }));
    const link = page.getByRole("link", { name: "Continue authentication" });
    link.element().addEventListener("click", (event) => event.preventDefault());
    await userEvent.click(link);

    expect(state.acceptUrlAuth).toHaveBeenCalledWith({
      environmentId,
      input: { instanceId, elicitationId: action.elicitationId },
    });
  });
});
