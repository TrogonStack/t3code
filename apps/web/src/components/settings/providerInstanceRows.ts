import { useAtomValue } from "@effect/atom-react";
import {
  defaultInstanceIdForDriver,
  type EnvironmentId,
  ProviderDriverKind,
  type ProviderInstanceConfig,
  type ProviderInstanceId,
  type ServerProvider,
  type ServerSettings,
  resolveProviderInstanceEnabled,
} from "@t3tools/contracts";

import { useEnvironmentSettings } from "../../hooks/useSettings";
import { providersIgnoringCheckedAtAtom } from "../../state/server";
import { providerClients } from "./providerDriverMeta";
import type { ProviderInstanceOption } from "./providerProfiles.logic";

export interface ProviderInstanceRowData {
  readonly instanceId: ProviderInstanceId;
  /** Undefined for a default slot the user never configured. */
  readonly explicitInstance: ProviderInstanceConfig | undefined;
  readonly driver: ProviderDriverKind;
  readonly isDefault: boolean;
  readonly isDirty?: boolean;
}

export function providerInstanceName(instance: ProviderInstanceConfig): string {
  return (
    instance.displayName?.trim() ||
    providerClients.get(instance.driver)?.label ||
    String(instance.driver)
  );
}

const PROVIDER_SETTINGS = providerClients.definitions.map((definition) => ({
  provider: definition.driverKind,
  hasDefaultInstance: definition.hasDefaultInstance !== false,
}));

/** The provider instances the Providers list shows, in its order. */
export function buildProviderInstanceRows(
  settings: Pick<ServerSettings, "providerInstances">,
  serverProviders: ReadonlyArray<ServerProvider>,
  targetInstanceId?: ProviderInstanceId,
): ProviderInstanceRowData[] {
  const visibleProviderSettings = PROVIDER_SETTINGS.filter(
    (providerSettings) =>
      providerSettings.provider !== "cursor" ||
      serverProviders.some(
        (provider) =>
          provider.instanceId === defaultInstanceIdForDriver(ProviderDriverKind.make("cursor")),
      ),
  );
  const instancesByDriver = new Map<
    ProviderDriverKind,
    Array<[ProviderInstanceId, ProviderInstanceConfig]>
  >();
  for (const [rawId, instance] of Object.entries(settings.providerInstances ?? {})) {
    const driver = instance.driver;
    const list = instancesByDriver.get(driver) ?? [];
    list.push([rawId as ProviderInstanceId, instance]);
    instancesByDriver.set(driver, list);
  }

  const defaultSlotIdsBySource = new Set<string>(
    visibleProviderSettings.map((providerSettings) =>
      String(defaultInstanceIdForDriver(providerSettings.provider)),
    ),
  );

  const rows: ProviderInstanceRowData[] = [];
  const visibleDriverKinds = new Set<ProviderDriverKind>(
    visibleProviderSettings.map((providerSettings) => providerSettings.provider),
  );

  for (const providerSettings of visibleProviderSettings) {
    const driver = providerSettings.provider;
    const defaultInstanceId = defaultInstanceIdForDriver(driver);
    const explicitInstance = settings.providerInstances?.[defaultInstanceId];
    const isDirty = explicitInstance !== undefined;
    const hasDefaultSlot = providerSettings.hasDefaultInstance || explicitInstance !== undefined;
    if (
      hasDefaultSlot &&
      (driver === "codex" ||
        driver === "claudeAgent" ||
        isDirty ||
        resolveProviderInstanceEnabled(explicitInstance ?? { driver }) ||
        defaultInstanceId === targetInstanceId)
    ) {
      rows.push({
        instanceId: defaultInstanceId,
        explicitInstance,
        driver,
        isDefault: true,
        isDirty,
      });
    }
    for (const [id, instance] of instancesByDriver.get(providerSettings.provider) ?? []) {
      if (id === defaultInstanceId) continue;
      rows.push({
        instanceId: id,
        explicitInstance: instance,
        driver: instance.driver,
        isDefault: false,
      });
    }
  }
  for (const [driver, list] of instancesByDriver) {
    if (visibleDriverKinds.has(driver)) continue;
    for (const [id, instance] of list) {
      rows.push({
        instanceId: id,
        explicitInstance: instance,
        driver: instance.driver,
        isDefault: defaultSlotIdsBySource.has(String(id)),
      });
    }
  }
  return rows;
}

function providerInstanceOptions(
  rows: ReadonlyArray<ProviderInstanceRowData>,
  serverProviders: ReadonlyArray<ServerProvider>,
): ProviderInstanceOption[] {
  return rows.map((row) => ({
    instanceId: row.instanceId,
    label:
      serverProviders
        .find((candidate) => candidate.instanceId === row.instanceId)
        ?.displayName?.trim() ||
      providerInstanceName(row.explicitInstance ?? { driver: row.driver }),
  }));
}

export function useProviderInstanceOptions(environmentId: EnvironmentId) {
  const settings = useEnvironmentSettings(environmentId);
  const serverProviders = useAtomValue(providersIgnoringCheckedAtAtom(environmentId));
  return providerInstanceOptions(
    buildProviderInstanceRows(settings, serverProviders),
    serverProviders,
  );
}
