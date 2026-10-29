// Preload installs desktopBridge before web code runs, so module-load detection is reliable.
export const isElectron = typeof window !== "undefined" && window.desktopBridge !== undefined;

// The fork ships Secrets unconditionally; upstream gates it behind T3CODE_FEATURE_SECRETS.
export const isSecretsFeatureEnabled = true;
