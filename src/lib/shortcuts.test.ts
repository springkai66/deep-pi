import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  SHORTCUTS, hostCommandEnabled, hostShortcutDecision, shortcutLabel, shortcutAria, shortcutKeymap,
  type HostShortcutContext, type ShortcutEvent, type HostCommand,
} from "./shortcuts";

const context: HostShortcutContext = {
  pi: true, workspace: true, rpc: true, fileOpen: true, fileReady: true,
  fileBusy: false, navigationLocked: false, modal: false, recovery: false, closing: false,
};
const event = (key: string, extra: Partial<ShortcutEvent> = {}): ShortcutEvent => ({
  key, ctrlKey: true, metaKey: false, altKey: false, shiftKey: false,
  isComposing: false, repeat: false, ...extra,
});

describe("shared shortcuts", () => {
  it("keeps the documented chord list synchronized with the registry", () => {
    const document = readFileSync(new URL("../../KEYBOARD_SHORTCUTS.md", import.meta.url), "utf8");
    for (const id of Object.keys(SHORTCUTS) as (keyof typeof SHORTCUTS)[]) {
      expect(document).toContain(`| ${shortcutLabel(id)} |`);
    }
  });
  it("derives menu labels, accessible chords and CodeMirror keys from the same definition", () => {
    expect(shortcutLabel("tasks")).toBe("Ctrl+Shift+P");
    expect(shortcutAria("tasks")).toBe("Control+Shift+P Meta+Shift+P");
    expect(shortcutKeymap("saveAs")).toBe("Mod-Shift-s");
    expect(shortcutLabel("previousDiff")).toBe("Shift+F7");
    expect(shortcutKeymap("nextDiff")).toBe("F7");
    expect(new Set(Object.keys(SHORTCUTS).map((id) => shortcutLabel(id as keyof typeof SHORTCUTS))).size)
      .toBe(Object.keys(SHORTCUTS).length);
  });

  it("executes each advertised host shortcut", () => {
    for (const [id, definition] of Object.entries(SHORTCUTS)) {
      if (definition.scope === "diff") continue;
      const result = hostShortcutDecision(event(definition.key, { shiftKey: definition.shift }), context);
      expect(result).toEqual({ consume: true, command: id });
      expect(hostCommandEnabled(id as HostCommand, context)).toBe(true);
    }
  });

  it("ignores IME, legacy composition, AltGraph and unsupported modifier combinations", () => {
    for (const extra of [
      { isComposing: true }, { keyCode: 229 }, { altKey: true }, { metaKey: true },
      { getModifierState: (key: string) => key === "AltGraph" }, { defaultPrevented: true },
    ]) expect(hostShortcutDecision(event("s", extra), context)).toEqual({ consume: false, command: null });
    expect(hostShortcutDecision(event("b", { shiftKey: true }), context).consume).toBe(false);
  });

  it("consumes long-press repeats without repeatedly invoking commands", () => {
    for (const key of ["s", "p", "b", ",", "l"]) {
      expect(hostShortcutDecision(event(key, { repeat: true }), context)).toEqual({ consume: true, command: null });
    }
  });

  it("preserves the existing Meta modifier alias without accepting Ctrl+Meta together", () => {
    expect(hostShortcutDecision(event("p", { ctrlKey: false, metaKey: true }), context).command).toBe("files");
    expect(hostShortcutDecision(event("p", { metaKey: true }), context).consume).toBe(false);
  });

  it("never treats bare typing or Shift-only letters as host commands", () => {
    for (const key of ["p", "s", "l", "b", ","]) {
      for (const shiftKey of [false, true]) {
        expect(hostShortcutDecision(event(key, { ctrlKey: false, shiftKey }), context))
          .toEqual({ consume: false, command: null });
      }
    }
    for (const ctrlKey of [false, true]) {
      for (const metaKey of [false, true]) {
        expect(hostShortcutDecision(event("p", { ctrlKey, metaKey }), context).command)
          .toBe(ctrlKey !== metaKey ? "files" : null);
      }
    }
  });

  it("keeps terminal control keys and diff keys outside the host router", () => {
    const terminal = { ...context, rpc: false, fileOpen: false, fileReady: false };
    expect(hostShortcutDecision(event("l"), terminal).consume).toBe(false);
    expect(hostShortcutDecision(event("s"), terminal).consume).toBe(false);
    expect(hostShortcutDecision(event("F7", { ctrlKey: false }), context).consume).toBe(false);
    expect(hostShortcutDecision(event("p"), terminal).command).toBe("files");
  });

  it("suppresses native page save while the active file is loading or saving", () => {
    for (const patch of [{ fileBusy: true }, { fileReady: false }]) {
      const busy = { ...context, ...patch };
      expect(hostCommandEnabled("save", busy)).toBe(false);
      expect(hostShortcutDecision(event("s"), busy)).toEqual({ consume: true, command: null });
    }
  });

  it("uses identical menu and keyboard eligibility in settings and non-Pi views", () => {
    for (const state of [{ ...context, workspace: false }, { ...context, pi: false }]) {
      for (const [key, command] of [["l", "composer"], ["b", "sidebar"], ["s", "save"]] as const) {
        expect(hostCommandEnabled(command, state)).toBe(false);
        expect(hostShortcutDecision(event(key), state).command).toBeNull();
      }
      expect(hostCommandEnabled("settings", state)).toBe(true);
    }
  });

  it("respects dialogs, recovery, open menus and closing-window ownership", () => {
    for (const patch of [{ modal: true }, { recovery: true }]) {
      expect(hostShortcutDecision(event("s"), { ...context, ...patch }).consume).toBe(false);
      expect(hostCommandEnabled("save", { ...context, ...patch })).toBe(false);
    }
    expect(hostShortcutDecision(event("s"), context, true).consume).toBe(false);
    expect(hostShortcutDecision(event("a", { ctrlKey: false }), { ...context, closing: true }))
      .toEqual({ consume: true, command: null });
  });

  it("prevents navigation during protected settings operations without changing settings access", () => {
    const locked = { ...context, workspace: false, navigationLocked: true };
    expect(hostCommandEnabled("files", locked)).toBe(false);
    expect(hostCommandEnabled("tasks", locked)).toBe(false);
    expect(hostShortcutDecision(event("p"), locked)).toEqual({ consume: true, command: null });
    expect(hostCommandEnabled("settings", locked)).toBe(true);
  });
});
