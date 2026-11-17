import { createFileRoute } from "@tanstack/react-router";

import { ProviderProfilesPage } from "../components/settings/ProviderProfilesSettings";
import { useSettingsScope } from "../components/settings/SettingsScopeContext";

function SettingsProviderProfilesRoute() {
  const { environment, scope } = useSettingsScope();
  if (!environment) {
    return (
      <p className="p-8 text-sm text-muted-foreground">
        {scope.kind === "environment"
          ? `Reconnect ${scope.label} to set up its provider profiles.`
          : "Connect an environment to set up its provider profiles."}
      </p>
    );
  }
  return <ProviderProfilesPage environmentId={environment.environmentId} />;
}

export const Route = createFileRoute("/_chat/settings/provider-profiles")({
  component: SettingsProviderProfilesRoute,
});
