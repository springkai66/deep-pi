export const SHORTCUTS = {
  tasks: { key: "p", primary: true, shift: true, scope: "host" },
  files: { key: "p", primary: true, shift: false, scope: "host" },
  settings: { key: ",", primary: true, shift: false, scope: "host" },
  sidebar: { key: "b", primary: true, shift: false, scope: "workspace" },
  composer: { key: "l", primary: true, shift: false, scope: "composer" },
  save: { key: "s", primary: true, shift: false, scope: "editor" },
  saveAs: { key: "s", primary: true, shift: true, scope: "editor" },
  nextDiff: { key: "F7", primary: false, shift: false, scope: "diff" },
  previousDiff: { key: "F7", primary: false, shift: true, scope: "diff" },
} as const;

export type ShortcutId = keyof typeof SHORTCUTS;
export type HostCommand = Exclude<ShortcutId, "nextDiff" | "previousDiff">;
export type ShortcutEvent = Pick<KeyboardEvent,
  "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey" | "isComposing" | "repeat">
  & Partial<Pick<KeyboardEvent, "keyCode" | "defaultPrevented" | "getModifierState">>;

export function shortcutLabel(id: ShortcutId): string {
  const shortcut = SHORTCUTS[id];
  return [shortcut.primary ? "Ctrl" : "", shortcut.shift ? "Shift" : "", shortcut.key.toUpperCase()].filter(Boolean).join("+");
}

export function shortcutAria(id: ShortcutId): string {
  const shortcut = SHORTCUTS[id];
  const key = [shortcut.shift ? "Shift" : "", shortcut.key.toUpperCase()].filter(Boolean).join("+");
  return shortcut.primary ? `Control+${key} Meta+${key}` : key;
}

export function shortcutKeymap(id: ShortcutId): string {
  const shortcut = SHORTCUTS[id];
  return [shortcut.primary ? "Mod" : "", shortcut.shift ? "Shift" : "", shortcut.key].filter(Boolean).join("-");
}

export function matchHostShortcut(event: ShortcutEvent): HostCommand | null {
  if (event.isComposing || event.keyCode === 229 || event.defaultPrevented || event.altKey
    || event.getModifierState?.("AltGraph") || event.ctrlKey === event.metaKey) return null;
  const key = event.key.toLowerCase();
  for (const [id, shortcut] of Object.entries(SHORTCUTS)) {
    if (shortcut.scope !== "diff" && key === shortcut.key && event.shiftKey === shortcut.shift) return id as HostCommand;
  }
  return null;
}

export interface HostShortcutContext {
  pi: boolean;
  workspace: boolean;
  rpc: boolean;
  fileOpen: boolean;
  fileReady: boolean;
  fileBusy: boolean;
  navigationLocked: boolean;
  modal: boolean;
  recovery: boolean;
  closing: boolean;
}

function inScope(command: HostCommand, context: HostShortcutContext): boolean {
  switch (SHORTCUTS[command].scope) {
    case "host": return true;
    case "workspace": return context.pi && context.workspace;
    case "composer": return context.pi && context.workspace && context.rpc;
    case "editor": return context.pi && context.workspace && context.fileOpen;
  }
}

export function hostCommandEnabled(command: HostCommand, context: HostShortcutContext): boolean {
  if (context.closing || context.modal || context.recovery || !inScope(command, context)) return false;
  if ((command === "tasks" || command === "files") && context.navigationLocked) return false;
  if (command === "save" || command === "saveAs") return context.fileReady && !context.fileBusy;
  return true;
}

export function hostShortcutDecision(event: ShortcutEvent, context: HostShortcutContext, menuOpen = false): {
  consume: boolean; command: HostCommand | null;
} {
  if (context.closing) return { consume: true, command: null };
  if (context.modal || context.recovery || menuOpen) return { consume: false, command: null };
  const command = matchHostShortcut(event);
  if (!command || !inScope(command, context)) return { consume: false, command: null };
  // Reserved keys must not fall through to WebView page-save/print actions while an editor is busy.
  return { consume: true, command: !event.repeat && hostCommandEnabled(command, context) ? command : null };
}
