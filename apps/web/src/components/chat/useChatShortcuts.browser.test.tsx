import type { KeybindingCommand, ResolvedKeybindingsConfig } from "@t3tools/contracts";
import { useEffect, useState, type ReactNode } from "react";
import { expect, it } from "vite-plus/test";
import { userEvent } from "vite-plus/test/browser";

import { surfaceShortcutKey } from "~/lib/surfaceShortcutKey";
import { render } from "../../../test/render";
import type { ChatComposerHandle } from "./ChatComposer";
import { useChatShortcuts } from "./useChatShortcuts";

const shortcutContext = {
  terminalFocus: false,
  terminalOpen: false,
  previewFocus: false,
  previewOpen: false,
  isWeb: true,
  isDesktop: false,
  modelPickerOpen: false,
};

const focusOnMount = (element: HTMLElement | null) => element?.focus();

// Stands in for the right panel's launcher, which needs the whole app around it. Like the
// real one it takes focus, advertises its letters in `data-surface-launcher-keys`, and
// claims one from its own window capture listener unless the key was already handled.
function Launcher(props: { keys: string; received: string[] }) {
  useEffect(() => {
    const claim = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const key = surfaceShortcutKey(event);
      if (key === null || !props.keys.toLowerCase().includes(key)) return;
      event.preventDefault();
      props.received.push(event.key);
    };
    window.addEventListener("keydown", claim, true);
    return () => window.removeEventListener("keydown", claim, true);
  }, [props.keys, props.received]);
  return (
    <div
      ref={focusOnMount}
      tabIndex={0}
      aria-label="Open a surface"
      data-surface-launcher-keys={props.keys}
    />
  );
}

// The chat's shortcut listener registers when the chat mounts, so a launcher that opens
// later registers after it. Tests mount the chat first and add the launcher by rerendering.
function Chat(props: {
  composerKeys: string[];
  composerFocuses?: string[];
  launcher?: { keys: string; received: string[] };
  children?: ReactNode;
}) {
  useChatShortcuts({
    enabled: true,
    keybindings: [],
    scriptKeybindings: [],
    getShortcutContext: () => shortcutContext,
    threadFindActive: false,
    // Type-to-focus and Escape-to-focus only call these, so the stand-in composer implements just them.
    composerRef: {
      current: {
        insertTextAtEnd: (key: string) => {
          props.composerKeys.push(key);
          return true;
        },
        focusAtEnd: () => {
          props.composerFocuses?.push("end");
        },
      } as Partial<ChatComposerHandle> as ChatComposerHandle,
    },
    branchToolbarRef: { current: null },
    closeThreadFind: () => {},
    commands: {},
    fallback: () => undefined,
  });
  return (
    <>
      {props.launcher ? <Launcher {...props.launcher} /> : null}
      {props.children}
    </>
  );
}

// router: false because the chat shortcuts and the launcher stand-in never navigate.
const options = { router: false } as const;

it("leaves the launcher's letters to the launcher, in either case", async () => {
  const composerKeys: string[] = [];
  const received: string[] = [];
  await using screen = await render(<Chat composerKeys={composerKeys} />, options);
  await screen.rerender(<Chat composerKeys={composerKeys} launcher={{ keys: "BT", received }} />);

  await userEvent.keyboard("tT");

  expect(received).toEqual(["t", "T"]);
  expect(composerKeys).toEqual([]);
});

it("still types other letters into the composer while the launcher is open", async () => {
  const composerKeys: string[] = [];
  const received: string[] = [];
  await using screen = await render(<Chat composerKeys={composerKeys} />, options);
  await screen.rerender(<Chat composerKeys={composerKeys} launcher={{ keys: "BT", received }} />);

  await userEvent.keyboard("a");

  expect(composerKeys).toEqual(["a"]);
  expect(received).toEqual([]);
});

it("focuses the composer on Escape when nothing has focus", async () => {
  const composerFocuses: string[] = [];
  await using _screen = await render(
    <Chat composerKeys={[]} composerFocuses={composerFocuses} />,
    options,
  );

  await userEvent.keyboard("{Escape}");

  expect(composerFocuses).toEqual(["end"]);
});

it("leaves Escape to a focused field", async () => {
  const composerFocuses: string[] = [];
  await using _screen = await render(
    <Chat composerKeys={[]} composerFocuses={composerFocuses}>
      <input ref={focusOnMount} aria-label="Rename" />
    </Chat>,
    options,
  );

  await userEvent.keyboard("{Escape}");

  expect(composerFocuses).toEqual([]);
});

it("leaves Escape alone while text is selected", async () => {
  const composerFocuses: string[] = [];
  await using screen = await render(
    <Chat composerKeys={[]} composerFocuses={composerFocuses}>
      <p>Assistant reply</p>
    </Chat>,
    options,
  );
  await userEvent.tripleClick(screen.getByText("Assistant reply"));

  await userEvent.keyboard("{Escape}");

  expect(composerFocuses).toEqual([]);
});

it("leaves Escape to a fullscreen MCP app so its popover can light-dismiss", async () => {
  const composerFocuses: string[] = [];
  await using screen = await render(
    <Chat composerKeys={[]} composerFocuses={composerFocuses}>
      <div popover="auto" data-mcp-app-fullscreen="" data-testid="fullscreen-app">
        App
      </div>
    </Chat>,
    options,
  );
  const popover = screen.getByTestId("fullscreen-app").element() as HTMLElement;
  popover.showPopover();

  await userEvent.keyboard("{Escape}");

  expect(composerFocuses).toEqual([]);
  expect(popover.matches(":popover-open")).toBe(false);
});

it("leaves Escape alone while an MCP app is fullscreen without native popovers", async () => {
  const composerFocuses: string[] = [];
  await using _screen = await render(
    <Chat composerKeys={[]} composerFocuses={composerFocuses}>
      <div data-mcp-app-fullscreen="">App</div>
    </Chat>,
    options,
  );

  await userEvent.keyboard("{Escape}");

  expect(composerFocuses).toEqual([]);
});

it("leaves Escape alone once something else handled it", async () => {
  const composerFocuses: string[] = [];
  await using _screen = await render(
    <Chat composerKeys={[]} composerFocuses={composerFocuses} />,
    options,
  );
  using _listener = claimEscape();

  await userEvent.keyboard("{Escape}");

  expect(composerFocuses).toEqual([]);
});

function claimEscape(): Disposable {
  const claim = (event: KeyboardEvent) => event.preventDefault();
  window.addEventListener("keydown", claim, true);
  return { [Symbol.dispose]: () => window.removeEventListener("keydown", claim, true) };
}
it("leaves a non-Latin launcher letter to the launcher by its physical key", async () => {
  const composerKeys: string[] = [];
  const received: string[] = [];
  await using screen = await render(<Chat composerKeys={composerKeys} />, options);
  await screen.rerender(<Chat composerKeys={composerKeys} launcher={{ keys: "BT", received }} />);

  // Playwright cannot press a key its US layout lacks, so send what a Ukrainian T key sends.
  const event = new KeyboardEvent("keydown", {
    key: "\u0435",
    code: "KeyT",
    bubbles: true,
    cancelable: true,
  });
  (document.activeElement ?? document.body).dispatchEvent(event);

  expect(composerKeys).toEqual([]);
  expect(received).toEqual(["\u0435"]);
});

it.each([
  { key: "b", code: "KeyN", composer: [], launcher: ["b"] },
  { key: "x", code: "KeyB", composer: ["x"], launcher: [] },
  { key: "è", code: "KeyT", composer: ["è"], launcher: [] },
])("routes $key on $code by the typed letter", async ({ key, code, composer, launcher }) => {
  const composerKeys: string[] = [];
  const received: string[] = [];
  await using screen = await render(<Chat composerKeys={composerKeys} />, options);
  await screen.rerender(<Chat composerKeys={composerKeys} launcher={{ keys: "BT", received }} />);

  const event = new KeyboardEvent("keydown", { key, code, bubbles: true, cancelable: true });
  (document.activeElement ?? document.body).dispatchEvent(event);

  expect(composerKeys).toEqual(composer);
  expect(received).toEqual(launcher);
});

const ctrlShift = (key: string, command: KeybindingCommand) => ({
  command,
  shortcut: { key, metaKey: false, ctrlKey: true, shiftKey: true, altKey: false, modKey: false },
});
const sideChatKeybindings: ResolvedKeybindingsConfig = [
  ctrlShift("m", "modelPicker.toggle"),
  ctrlShift("x", "thread.stop"),
];

// A side chat in the main chat's panel: no window listeners, only its composer's own chords.
function SideChat(props: { toggles: string[]; stops: string[] }) {
  const [composerElement, setComposerElement] = useState<HTMLDivElement | null>(null);
  useChatShortcuts({
    enabled: false,
    embedded: true,
    composerElement,
    keybindings: sideChatKeybindings,
    scriptKeybindings: [],
    getShortcutContext: () => shortcutContext,
    threadFindActive: false,
    composerRef: {
      current: {
        toggleModelPicker: () => props.toggles.push("model"),
      } as Partial<ChatComposerHandle> as ChatComposerHandle,
    },
    branchToolbarRef: { current: null },
    closeThreadFind: () => {},
    commands: { "thread.stop": () => void props.stops.push("stop") },
    fallback: () => undefined,
  });
  return (
    <>
      <div ref={setComposerElement}>
        <textarea aria-label="Side chat composer" />
      </div>
      <textarea aria-label="Main composer" />
    </>
  );
}

it("runs a side chat's composer chords only from its composer, and never thread commands", async () => {
  const toggles: string[] = [];
  const stops: string[] = [];
  await using screen = await render(<SideChat toggles={toggles} stops={stops} />, options);

  await userEvent.click(screen.getByLabelText("Side chat composer"));
  await userEvent.keyboard("{Control>}{Shift>}MX{/Shift}{/Control}");
  await userEvent.click(screen.getByLabelText("Main composer"));
  await userEvent.keyboard("{Control>}{Shift>}M{/Shift}{/Control}");

  expect(toggles).toEqual(["model"]);
  expect(stops).toEqual([]);
});
