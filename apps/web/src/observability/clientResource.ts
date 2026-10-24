import { releaseResourceAttributes } from "@t3tools/shared/releaseResource";

interface NavigatorUserAgentData {
  readonly platform: string;
  readonly mobile: boolean;
  readonly brands: ReadonlyArray<{ readonly brand: string; readonly version: string }>;
}

type BrowserNavigator = Pick<Navigator, "userAgent" | "language"> & {
  readonly userAgentData?: NavigatorUserAgentData;
};

/**
 * The same `browser.*` and `user_agent.original` attributes the OpenTelemetry
 * browser resource detector reports, plus the `process.runtime.*` values the
 * semantic conventions give a web browser. `userAgentData` exists only in
 * Chromium, which includes the desktop app.
 */
const browserResourceAttributes = (
  navigator: BrowserNavigator | undefined,
): Record<string, unknown> => {
  if (navigator === undefined) return {};
  const { userAgentData } = navigator;
  return {
    "user_agent.original": navigator.userAgent,
    "process.runtime.name": "browser",
    "process.runtime.version": navigator.userAgent,
    "browser.language": navigator.language,
    ...(userAgentData && {
      "browser.platform": userAgentData.platform,
      "browser.mobile": userAgentData.mobile,
      "browser.brands": userAgentData.brands.map(({ brand, version }) => `${brand} ${version}`),
    }),
  };
};

/**
 * The UI is a build of its own, so it reports its own release identity rather
 * than the server's. Vercel PR previews cannot carry it in `version` without
 * breaking the version-skew comparison against the server, so they mark
 * themselves with `VITE_RELEASE_CHANNEL=preview`.
 */
export const clientTracingResource = (input: {
  readonly version: string;
  readonly isDevelopment: boolean;
  readonly releaseChannel: string | undefined;
  readonly surface: "desktop" | "web";
  readonly serviceInstanceId: string;
  readonly navigator: BrowserNavigator | undefined;
}): {
  readonly serviceName: string;
  readonly serviceVersion: string;
  readonly attributes: Record<string, unknown>;
} => ({
  serviceName: "t3code-web",
  serviceVersion: input.version,
  attributes: {
    "service.namespace": "t3code",
    "service.instance.id": input.serviceInstanceId,
    ...releaseResourceAttributes({
      version: input.version,
      isDevelopment: input.isDevelopment,
      operatorAttributes: {},
      channel: input.releaseChannel?.trim().toLowerCase() === "preview" ? "preview" : undefined,
    }),
    "t3code.client.surface": input.surface,
    ...browserResourceAttributes(input.navigator),
  },
});
