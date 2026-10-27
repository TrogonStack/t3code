import { expect, it } from "vite-plus/test";

import { render } from "../../../test/render";
import type { AssistantCitationRequest } from "./AssistantCitationSource";
import { useAssistantCitationTarget } from "./useAssistantCitationTarget";

const request = { key: "citation-1", citation: {} } as AssistantCitationRequest;

function Citation() {
  useAssistantCitationTarget({
    request,
    phase: null,
    viewport: null,
    onManualNavigation: () => {},
  });
  return null;
}

function pressEscape() {
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  document.body.dispatchEvent(event);
  return event;
}

it("claims only the Escape that dismisses the citation", async () => {
  // router: false because the hook never navigates.
  await using _screen = await render(<Citation />, { router: false });

  expect(pressEscape().defaultPrevented).toBe(true);
  expect(pressEscape().defaultPrevented).toBe(false);
});
