import * as Layer from "effect/Layer";
import * as Tracer from "effect/Tracer";
import { releaseChannelOf } from "@t3tools/shared/releaseResource";

import { randomUUID } from "../lib/utils";
import { APP_VERSION } from "~/branding";

/** `service.instance.id` for this page load, shared by every resource the UI exports. */
export const serviceInstanceId = randomUUID();

// PR previews can't mark themselves in APP_VERSION without breaking the version-skew check against the server.
export const releaseChannel =
  import.meta.env.DEV || import.meta.env.VITE_RELEASE_CHANNEL === "dev"
    ? "dev"
    : import.meta.env.VITE_RELEASE_CHANNEL === "preview"
      ? "preview"
      : releaseChannelOf(APP_VERSION);

let delegate: Tracer.Tracer | null = null;

export function setDelegate(next: Tracer.Tracer | null): void {
  delegate = next;
}

export function hasDelegate(): boolean {
  return delegate !== null;
}

// Install before exporters exist so spans reach whichever exporter is configured later.
export const layer = Layer.succeed(
  Tracer.Tracer,
  Tracer.make({
    span(options) {
      return delegate?.span(options) ?? new Tracer.NativeSpan(options);
    },
  }),
);
